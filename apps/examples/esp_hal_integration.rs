//! esp-hal Integration Example
//!
//! This example demonstrates using ph-esp32-mac with the esp-hal ecosystem.
//! It emphasizes a low-boilerplate bring-up path with esp-hal types.
//!
//! # Features Demonstrated
//!
//! - WT32-ETH01 board helpers (`EmacBuilder`, `EmacPhyBundle`)
//! - Interrupt-safe shared EMAC access
//! - Link polling and packet sniffing
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
//! cargo xtask run ex-esp-hal
//! ```
//!
//! # Features Required
//!
//! Enable the `esp-hal` feature in Cargo.toml:
//! ```toml
//! ph-esp32-mac = { version = "0.1", features = ["esp32", "esp-hal", "critical-section"] }
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main, time,
};
use log::{error, info};

use ph_esp32_mac::esp_hal::{EmacBuilder, EmacPhyBundle, Wt32Eth01};
use ph_esp32_mac::{Duplex, Emac, Speed};

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
// Static EMAC Instance
// =============================================================================

// Thread-safe EMAC instance wrapped for interrupt-safe access
ph_esp32_mac::emac_static_sync!(EMAC, 10, 10, 1600);

// =============================================================================
// Main Entry Point
// =============================================================================

esp_app_desc!();

#[main]
fn main() -> ! {
    // Initialize esp-hal
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize logging
    esp_println::logger::init_logger_from_env();
    info!("ph-esp32-mac esp-hal example starting...");

    // Create delay provider from esp-hal
    let mut delay = Delay::new();

    // Enable external 50 MHz oscillator (WT32-ETH01 specific).
    // GPIO16 controls the oscillator enable on this board.
    let mut clk_en = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    clk_en.set_high();
    info!(
        "External oscillator enabled (GPIO{} = HIGH)",
        Wt32Eth01::CLK_EN_GPIO
    );

    // Small delay for oscillator startup
    delay.delay_millis(Wt32Eth01::OSC_STARTUP_MS);

    info!("Initializing EMAC...");
    EMAC.with(|emac| {
        match EmacBuilder::wt32_eth01_with_mac(emac, MAC_ADDRESS)
            .init(&mut delay)
        {
            Ok(_) => info!("EMAC initialized successfully"),
            Err(e) => {
                error!("EMAC initialization failed: {:?}", e);
                panic!("Cannot continue without EMAC");
            }
        }
    });

    // Initialize PHY and wait for link (WT32-ETH01 LAN8720A).
    info!("Initializing LAN8720A PHY...");
    info!("Waiting for Ethernet link...");
    let link_status = match EMAC.with(|emac| {
        let mut emac_phy = EmacPhyBundle::wt32_eth01_lan8720a(emac, Delay::new());
        emac_phy.init_and_wait_link_up(&mut delay, LINK_TIMEOUT_MS, LINK_POLL_MS)
    }) {
        Ok(status) => status,
        Err(err) => {
            error!("Link wait failed: {:?}", err);
            panic!("No Ethernet link");
        }
    };

    info!(
        "Link UP: {} {}",
        match link_status.speed {
            Speed::Mbps10 => "10 Mbps",
            Speed::Mbps100 => "100 Mbps",
        },
        match link_status.duplex {
            Duplex::Half => "Half Duplex",
            Duplex::Full => "Full Duplex",
        }
    );

    // Start the EMAC
    info!("Starting EMAC...");
    EMAC.with(|emac| match emac.start() {
        Ok(()) => info!("EMAC started successfully"),
        Err(e) => {
            error!("Failed to start EMAC: {:?}", e);
            panic!("Cannot start EMAC");
        }
    });

    info!("Ethernet is ready! Running packet sniffer...");
    info!(
        "Memory usage: {} bytes",
        Emac::<10, 10, 1600>::memory_usage()
    );

    // Main loop: simple frame echo with statistics
    let mut rx_buffer = [0u8; 1600];
    let mut frames_rx = 0u32;
    let mut errors = 0u32;
    let mut last_stats_time = time::Instant::now();

    loop {
        EMAC.with(|emac| {
            if emac.rx_available() {
                // Try to receive a frame
                match emac.receive(&mut rx_buffer) {
                    Ok(len) if len > 0 => {
                        frames_rx += 1;

                        if len < 14 {
                            info!("RX runt frame: {} bytes", len);
                        } else {
                            let ethertype = u16::from_be_bytes([rx_buffer[12], rx_buffer[13]]);
                            info!(
                                "RX {} bytes dst={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} src={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} type=0x{:04X}",
                                len,
                                rx_buffer[0],
                                rx_buffer[1],
                                rx_buffer[2],
                                rx_buffer[3],
                                rx_buffer[4],
                                rx_buffer[5],
                                rx_buffer[6],
                                rx_buffer[7],
                                rx_buffer[8],
                                rx_buffer[9],
                                rx_buffer[10],
                                rx_buffer[11],
                                ethertype
                            );
                        }
                    }
                    Err(ph_esp32_mac::Error::Io(ph_esp32_mac::IoError::IncompleteFrame)) => {}
                    Err(_) => errors += 1,
                    _ => {}
                }
            }
        });

        // Print statistics every 10 seconds
        let now = time::Instant::now();
        if (now - last_stats_time).as_secs() >= 10 {
            info!("Stats: RX={}, Errors={}", frames_rx, errors);
            last_stats_time = now;
        }

        // Small delay to prevent tight polling
        delay.delay_micros(10);
    }
}
