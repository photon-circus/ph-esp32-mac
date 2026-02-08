//! Embassy-net Async Networking Example
//!
//! This example demonstrates using ph-esp32-mac with embassy-net for async
//! TCP/IP networking on ESP32. It provides a UDP echo server accessible over
//! the network.
//!
//! # Features Demonstrated
//!
//! - Async executor via `esp-rtos` (recommended for esp-hal 1.0.0+)
//! - DHCP address acquisition
//! - UDP echo server on port 7
//! - Periodic PHY link monitoring
//! - Interrupt-driven packet handling
//!
//! # Hardware
//!
//! Tested on WT32-ETH01 board:
//! - ESP32 (WT32-S1 module)
//! - LAN8720A PHY at MDIO address 1
//! - External 50 MHz oscillator enabled via GPIO16
//!
//! # Building
//!
//! ```bash
//! cd examples
//! cargo run --bin embassy_net --features embassy-net-example --release
//! ```
//!
//! # Testing
//!
//! Once running, send UDP packets to port 7:
//! ```bash
//! echo "Hello ESP32!" | nc -u <DEVICE_IP> 7
//! ```
//!
//! # Architecture
//!
//! ```text
//!   ┌──────────────────────────────────────────────────────────────────────┐
//!   │                         Embassy Tasks                                │
//!   ├────────────┬─────────────┬──────────────┬────────────────────────────┤
//!   │ net_task   │ dhcp_task   │ udp_echo     │ link_task                  │
//!   │ (runner)   │ (config)    │ (application)│ (PHY polling)              │
//!   └─────┬──────┴──────┬──────┴──────┬───────┴──────────┬─────────────────┘
//!         │             │             │                  │
//!         └─────────────┴─────────────┴──────────────────┘
//!                               │
//!                  ┌────────────┴────────────┐
//!                  │  embassy_net::Stack     │
//!                  │  (TCP/IP processing)    │
//!                  └────────────┬────────────┘
//!                               │
//!                  ┌────────────┴────────────┐
//!                  │  EmbassyEmac Driver     │
//!                  │  (async TX/RX)          │
//!                  └────────────┬────────────┘
//!                               │
//!                  ┌────────────┴────────────┐
//!                  │  Emac + DMA             │
//!                  │  (hardware)             │
//!                  └────────────┬────────────┘
//!                               │
//!                  ┌────────────┴────────────┐
//!                  │  LAN8720A PHY           │
//!                  └─────────────────────────┘
//! ```

#![no_std]
#![no_main]

// =============================================================================
// Imports
// =============================================================================

use embassy_executor::Spawner;
use embassy_net::{udp::UdpSocket, Config, ConfigV4, DhcpConfig, Stack};
use embassy_net_driver::LinkState;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    interrupt::Priority,
    rng::Rng,
    timer::timg::TimerGroup,
};
use log::{info, warn};
use static_cell::StaticCell;

use ph_esp32_mac::esp_hal::{EmacBuilder, EmacExt, EmacPhyBundle, Wt32Eth01};
use ph_esp32_mac::hal::MdioController;
use ph_esp32_mac::{emac_isr, Emac, EmbassyEmac};

// =============================================================================
// Configuration
// =============================================================================

/// MAC address for this device (locally administered).
const MAC_ADDRESS: [u8; 6] = [0x02, 0x00, 0x00, 0x12, 0x34, 0x56];

/// UDP echo server port (standard echo protocol).
const UDP_ECHO_PORT: u16 = 7;

/// Maximum UDP packet size.
const UDP_BUFFER_SIZE: usize = 1024;

/// PHY link polling interval.
const LINK_POLL_MS: u64 = 500;

// =============================================================================
// Static Allocations
// =============================================================================

// EMAC hardware instance, driver state, and embassy-net resources.
// The macro places EMAC in DRAM and creates the required static cells.
ph_esp32_mac::embassy_net_statics!(EMAC, EMAC_STATE, NET_RESOURCES, 10, 10, 1600, 4);

// UDP socket buffers (must be static for embassy-net).
static UDP_RX_META: StaticCell<[embassy_net::udp::PacketMetadata; 4]> = StaticCell::new();
static UDP_TX_META: StaticCell<[embassy_net::udp::PacketMetadata; 4]> = StaticCell::new();
static UDP_RX_BUF: StaticCell<[u8; UDP_BUFFER_SIZE]> = StaticCell::new();
static UDP_TX_BUF: StaticCell<[u8; UDP_BUFFER_SIZE]> = StaticCell::new();

// =============================================================================
// Interrupt Handler
// =============================================================================

// Define the EMAC interrupt handler using the ISR macro.
// This wakes the embassy-net driver when packets arrive or complete.
emac_isr!(EMAC_IRQ, Priority::Priority1, {
    EMAC_STATE.handle_interrupt();
});

// =============================================================================
// Embassy Tasks
// =============================================================================

/// Network stack runner task.
///
/// This task runs the embassy-net stack, processing incoming/outgoing packets.
/// It must run continuously for the network to function.
#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, EmbassyEmac<'static, 10, 10, 1600>>) -> ! {
    runner.run().await
}

/// DHCP configuration task.
///
/// Waits for link-up, starts DHCP, and logs the acquired address.
/// Restarts DHCP if the configuration is lost.
#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) -> ! {
    loop {
        // Wait for physical link
        stack.wait_link_up().await;
        info!("Link up - starting DHCP");

        // Give the link a moment to stabilize
        Timer::after(Duration::from_secs(1)).await;

        // Start DHCP
        stack.set_config_v4(ConfigV4::Dhcp(DhcpConfig::default()));

        // Wait for address assignment
        loop {
            if let Some(config) = stack.config_v4() {
                info!("DHCP acquired: {}", config.address);
                if let Some(gw) = config.gateway {
                    info!("Gateway: {}", gw);
                }
                break;
            }
            Timer::after(Duration::from_secs(1)).await;
        }

        // Wait until config is lost (link down, lease expired, etc.)
        stack.wait_config_down().await;
        warn!("DHCP configuration lost");
    }
}

/// UDP echo server task.
///
/// Binds to UDP port 7 and echoes any received packets back to the sender.
#[embassy_executor::task]
async fn udp_echo_task(stack: Stack<'static>) -> ! {
    // Initialize socket buffers
    let rx_meta = UDP_RX_META.init([embassy_net::udp::PacketMetadata::EMPTY; 4]);
    let tx_meta = UDP_TX_META.init([embassy_net::udp::PacketMetadata::EMPTY; 4]);
    let rx_buf = UDP_RX_BUF.init([0u8; UDP_BUFFER_SIZE]);
    let tx_buf = UDP_TX_BUF.init([0u8; UDP_BUFFER_SIZE]);

    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);

    loop {
        // Wait for network configuration before binding
        stack.wait_config_up().await;

        if socket.bind(UDP_ECHO_PORT).is_err() {
            warn!("UDP bind failed, retrying...");
            Timer::after(Duration::from_secs(1)).await;
            continue;
        }
        info!("UDP echo server listening on port {}", UDP_ECHO_PORT);

        // Echo loop
        let mut buf = [0u8; UDP_BUFFER_SIZE];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, endpoint)) => {
                    info!("UDP: {} bytes from {}", len, endpoint);
                    if let Err(e) = socket.send_to(&buf[..len], endpoint).await {
                        warn!("UDP send error: {:?}", e);
                    }
                }
                Err(e) => {
                    warn!("UDP recv error: {:?}", e);
                    break; // Rebind on error
                }
            }
        }
    }
}

/// PHY link monitoring task.
///
/// Polls the PHY for link status changes and updates the driver state.
/// Also configures MAC speed/duplex when link comes up.
#[embassy_executor::task]
async fn link_task(emac_ptr: *mut Emac<10, 10, 1600>) -> ! {
    let mut mdio = MdioController::new(Delay::new());
    let mut phy = Wt32Eth01::lan8720a();
    let mut was_up = false;

    loop {
        // Poll PHY for link status
        match EMAC_STATE.update_link_from_phy(&mut phy, &mut mdio) {
            Ok(Some(status)) => {
                if !was_up {
                    info!("Link up: {:?} {:?}", status.speed, status.duplex);

                    // Update MAC speed/duplex to match PHY
                    // SAFETY: EMAC is static and we're the only task modifying speed/duplex
                    unsafe {
                        let emac = &mut *emac_ptr;
                        emac.set_speed(status.speed);
                        emac.set_duplex(status.duplex);
                    }
                    was_up = true;
                }
            }
            Ok(None) => {
                if was_up {
                    warn!("Link down");
                    was_up = false;
                }
            }
            Err(e) => {
                warn!("PHY error: {:?}", e);
            }
        }

        Timer::after(Duration::from_millis(LINK_POLL_MS)).await;
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // Initialize logging and peripherals
    esp_println::logger::init_logger_from_env();
    info!("Embassy-net example starting...");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Start embassy time driver (required for Timer)
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // -------------------------------------------------------------------------
    // Hardware Setup (WT32-ETH01 specific)
    // -------------------------------------------------------------------------

    // Enable external 50 MHz oscillator via GPIO16
    let mut clk_en = Output::new(peripherals.GPIO16, Level::High, OutputConfig::default());
    clk_en.set_high();
    let mut delay = Delay::new();
    delay.delay_millis(10);
    info!("External oscillator enabled (GPIO{})", Wt32Eth01::CLK_EN_GPIO);

    // -------------------------------------------------------------------------
    // EMAC Initialization
    // -------------------------------------------------------------------------

    // Initialize static EMAC instance
    let emac_ptr = EMAC.init(Emac::new()) as *mut Emac<10, 10, 1600>;

    // SAFETY: emac_ptr points to static storage valid for program lifetime
    let emac = unsafe { &mut *emac_ptr };

    // Configure and initialize EMAC for WT32-ETH01
    EmacBuilder::wt32_eth01_with_mac(emac, MAC_ADDRESS)
        .init(&mut delay)
        .expect("EMAC init failed");

    // Initialize PHY
    {
        let mut phy_bundle = EmacPhyBundle::wt32_eth01_lan8720a(emac, Delay::new());
        phy_bundle.init_phy().expect("PHY init failed");

        // Set initial link state
        let initial_link = match phy_bundle.link_status() {
            Ok(Some(status)) => {
                info!("Initial link: {:?} {:?}", status.speed, status.duplex);
                emac.set_speed(status.speed);
                emac.set_duplex(status.duplex);
                LinkState::Up
            }
            _ => {
                info!("Waiting for link...");
                LinkState::Down
            }
        };
        EMAC_STATE.set_link_state(initial_link);
    }

    // Start EMAC and bind interrupt handler
    emac.start().expect("EMAC start failed");
    emac.bind_interrupt(EMAC_IRQ);
    info!("EMAC started (memory: {} bytes)", Emac::<10, 10, 1600>::memory_usage());

    // -------------------------------------------------------------------------
    // Embassy-net Stack Setup
    // -------------------------------------------------------------------------

    // Create the embassy-net driver
    let driver = ph_esp32_mac::embassy_net_driver!(emac_ptr, &EMAC_STATE);

    // Generate random seed for TCP sequence numbers
    let rng = Rng::new();
    let seed = ((rng.random() as u64) << 32) | (rng.random() as u64);

    // Create network stack with static resources
    let (stack, runner) = ph_esp32_mac::embassy_net_stack!(driver, NET_RESOURCES, Config::default(), seed);

    // -------------------------------------------------------------------------
    // Spawn Tasks
    // -------------------------------------------------------------------------

    spawner.spawn(net_task(runner)).unwrap();
    spawner.spawn(dhcp_task(stack)).unwrap();
    spawner.spawn(udp_echo_task(stack)).unwrap();
    spawner.spawn(link_task(emac_ptr)).unwrap();

    info!("All tasks spawned - system running");

    // Keep main alive (tasks do all the work)
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
