//! esp-hal Async EMAC Example
//!
//! This example demonstrates async receive using `AsyncEmacState` with esp-hal.
//! It uses the embassy executor via `esp-rtos` and per-instance wakers for
//! efficient RX wakeups.
//!
//! # Features Demonstrated
//!
//! - Async RX via `AsyncEmacState`
//! - esp-rtos executor integration
//! - WT32-ETH01 bring-up helpers
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
//! cargo xtask run ex-esp-hal-async
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

use ph_esp32_mac::esp_hal::{
    emac_async_isr, EmacBuilder, EmacExt, EmacPhyBundle, Wt32Eth01,
};
use ph_esp32_mac::{AsyncEmacExt, Emac};

// =============================================================================
// Configuration
// =============================================================================

/// MAC address for this device (locally administered).
const MAC_ADDRESS: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x34, 0x56];

/// Link-up timeout (milliseconds).
const LINK_TIMEOUT_MS: u32 = 10_000;

/// Link poll interval (milliseconds).
const LINK_POLL_MS: u32 = 200;

// =============================================================================
// Static EMAC + Async State
// =============================================================================

ph_esp32_mac::emac_static_async!(EMAC, ASYNC_STATE, 10, 10, 1600);

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
    delay.delay_millis(Wt32Eth01::OSC_STARTUP_MS);
    info!(
        "External oscillator enabled (GPIO{} = HIGH)",
        Wt32Eth01::CLK_EN_GPIO
    );

    // Initialize EMAC in static storage.
    let emac_ptr = EMAC.init(Emac::new()) as *mut Emac<10, 10, 1600>;
    // SAFETY: EMAC is in static storage for the duration of the program.
    let emac = unsafe { &mut *emac_ptr };
    EmacBuilder::wt32_eth01_with_mac(emac, MAC_ADDRESS)
        .init(&mut delay)
        .unwrap();

    // Initialize PHY and wait for link.
    {
        let mut emac_phy = EmacPhyBundle::wt32_eth01_lan8720a(emac, Delay::new());
        match emac_phy.init_and_wait_link_up(&mut delay, LINK_TIMEOUT_MS, LINK_POLL_MS) {
            Ok(status) => info!("Link up: {:?}", status),
            Err(err) => warn!("Link wait failed: {:?}", err),
        }

        emac_phy.emac_mut().start().unwrap();
        emac_phy.emac_mut().bind_interrupt(EMAC_IRQ);
    }
    info!(
        "EMAC started; awaiting frames... (memory: {} bytes)",
        Emac::<10, 10, 1600>::memory_usage()
    );

    spawner.spawn(rx_task(emac_ptr)).unwrap();

    loop {
        core::future::pending::<()>().await;
    }
}
