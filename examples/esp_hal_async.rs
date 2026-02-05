//! esp-hal Async EMAC Example
//!
//! This example demonstrates async receive using `AsyncEmacState` with esp-hal.
//! It uses the embassy executor via `esp-rtos` and the per-instance async
//! waker state for efficient RX wakeups.
//!
//! # Hardware
//!
//! Tested on WT32-ETH01 board with:
//! - ESP32 (WT32-S1 module)
//! - LAN8720A PHY at address 1
//! - External 50 MHz oscillator (enabled via GPIO16)
//!
//! # Building
//!
//! ```bash
//! cargo build --bin esp_hal_async --target xtensa-esp32-none-elf --release \
//!     --features esp-hal-async-example
//! ```

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    interrupt::Priority,
    timer::timg::TimerGroup,
};
use log::{info, warn};
use static_cell::StaticCell;

use ph_esp32_mac::esp_hal::{EmacBuilder, EmacExt, EmacPhyBundle, emac_async_isr};
use ph_esp32_mac::{
    AsyncEmacExt, AsyncEmacState, Emac, EmacConfig, Lan8720a, MdioController, PhyInterface,
    RmiiClockMode,
};

// =============================================================================
// Board Configuration
// =============================================================================

/// PHY address (WT32-ETH01 has PHYAD0 pulled high = address 1).
const PHY_ADDR: u8 = 1;

/// GPIO for external oscillator enable.
const CLK_EN_GPIO: u8 = 16;

// =============================================================================
// Static EMAC + Async State
// =============================================================================

#[unsafe(link_section = ".dram1")]
static EMAC: StaticCell<Emac<10, 10, 1600>> = StaticCell::new();
static ASYNC_STATE: AsyncEmacState = AsyncEmacState::new();

emac_async_isr!(EMAC_IRQ, Priority::Priority1, &ASYNC_STATE);

// =============================================================================
// Async RX Task
// =============================================================================

#[embassy_executor::task]
async fn rx_task(emac: *mut Emac<10, 10, 1600>) -> ! {
    let mut rx_buf = [0u8; 1600];

    loop {
        // SAFETY: EMAC is in static storage for the program lifetime. This task
        // is the only async consumer of RX, so the mutable reference is exclusive
        // between await points.
        let result = unsafe { (&mut *emac).receive_async(&ASYNC_STATE, &mut rx_buf).await };

        match result {
            Ok(len) if len > 0 => {
                if len >= 14 {
                    let ethertype = u16::from_be_bytes([rx_buf[12], rx_buf[13]]);
                    info!("RX {} bytes, ethertype=0x{:04X}", len, ethertype);
                } else {
                    info!("RX {} bytes", len);
                }
            }
            Ok(_) => {}
            Err(err) => warn!("RX error: {:?}", err),
        }
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    info!("esp-hal async example starting...");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Start the embassy time driver (required by the esp-rtos executor).
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // Enable external oscillator (WT32-ETH01 specific).
    let mut clk_en = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    clk_en.set_high();
    let mut delay = Delay::new();
    delay.delay_millis(10);
    info!("External oscillator enabled (GPIO{} = HIGH)", CLK_EN_GPIO);

    // Initialize EMAC in static storage.
    let emac_ptr = EMAC.init(Emac::new()) as *mut Emac<10, 10, 1600>;
    let config = EmacConfig::rmii_esp32_default()
        .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
        .with_phy_interface(PhyInterface::Rmii)
        .with_rmii_clock(RmiiClockMode::ExternalInput { gpio: 0 });

    // SAFETY: EMAC is in static storage for the duration of the program.
    let emac = unsafe { &mut *emac_ptr };
    EmacBuilder::new(emac)
        .with_config(config)
        .init(&mut delay)
        .unwrap();

    // Initialize PHY and wait for link.
    {
        let mut emac_phy = EmacPhyBundle::new(
            emac,
            Lan8720a::new(PHY_ADDR),
            MdioController::new(Delay::new()),
        );
        emac_phy.init_phy().unwrap();

        match emac_phy.wait_link_up(&mut delay, 10_000, 200) {
            Ok(status) => info!("Link up: {:?}", status),
            Err(err) => warn!("Link wait failed: {:?}", err),
        }

        emac_phy.emac_mut().start().unwrap();
        emac_phy.emac_mut().bind_interrupt(EMAC_IRQ);
    }
    info!("EMAC started; awaiting frames...");

    spawner.spawn(rx_task(emac_ptr)).unwrap();

    loop {
        core::future::pending::<()>().await;
    }
}
