//! esp-hal Integration Example
//!
//! This example demonstrates using ph-esp32-mac with the esp-hal ecosystem.
//! It provides a more ergonomic experience with proper peripheral ownership,
//! interrupt handling, and delay types from esp-hal.
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
//! cargo build --bin esp_hal_integration --target xtensa-esp32-none-elf --release \
//!     --features esp-hal-example
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

use core::cell::RefCell;

use critical_section::Mutex;
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main, time,
};
use log::{error, info, warn};

use ph_esp32_mac::{
    Duplex, Emac, EmacConfig, Lan8720a, MdioController, PhyDriver, PhyInterface, RmiiClockMode,
    Speed,
};

// =============================================================================
// Board Configuration
// =============================================================================

/// PHY address (WT32-ETH01 has PHYAD0 pulled high = address 1)
const PHY_ADDR: u8 = 1;

/// GPIO for external oscillator enable
const CLK_EN_GPIO: u8 = 16;

// =============================================================================
// Static EMAC Instance
// =============================================================================

/// Thread-safe EMAC instance wrapped for interrupt-safe access
static EMAC: Mutex<RefCell<Option<Emac<10, 10, 1600>>>> = Mutex::new(RefCell::new(None));

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

    // Enable external 50 MHz oscillator (WT32-ETH01 specific)
    // GPIO16 controls the oscillator enable on this board
    let mut clk_en = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    clk_en.set_high();
    info!("External oscillator enabled (GPIO{} = HIGH)", CLK_EN_GPIO);

    // Small delay for oscillator startup
    delay.delay_millis(10);

    // Place EMAC in static storage before init (required for DMA descriptors).
    critical_section::with(|cs| {
        EMAC.borrow_ref_mut(cs).replace(Emac::new());
    });

    // Configure for WT32-ETH01 board
    let config = EmacConfig::new()
        .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
        .with_phy_interface(PhyInterface::Rmii)
        .with_rmii_clock(RmiiClockMode::ExternalInput { gpio: 0 });

    info!("Initializing EMAC...");
    critical_section::with(|cs| {
        let mut emac_ref = EMAC.borrow_ref_mut(cs);
        let emac = emac_ref.as_mut().expect("EMAC static unavailable");
        match emac.init(config, &mut delay) {
            Ok(()) => info!("EMAC initialized successfully"),
            Err(e) => {
                error!("EMAC initialization failed: {:?}", e);
                panic!("Cannot continue without EMAC");
            }
        }
    });

    // Initialize PHY
    info!("Initializing LAN8720A PHY at address {}...", PHY_ADDR);

    // Create MDIO controller with esp-hal delay
    let mut phy = Lan8720a::new(PHY_ADDR);
    let mut mdio = MdioController::new(Delay::new());

    match phy.init(&mut mdio) {
        Ok(()) => info!("PHY initialized successfully"),
        Err(e) => {
            error!("PHY initialization failed: {:?}", e);
            panic!("Cannot continue without PHY");
        }
    }

    // Check PHY ID
    match phy.phy_id(&mut mdio) {
        Ok(id) => info!("PHY ID: 0x{:08X}", id),
        Err(e) => warn!("Could not read PHY ID: {:?}", e),
    }

    // Wait for link to come up
    info!("Waiting for Ethernet link...");
    let mut link_up = false;

    for attempt in 0..50 {
        delay.delay_millis(200);

        match phy.poll_link(&mut mdio) {
            Ok(Some(status)) => {
                info!(
                    "Link UP: {} {}",
                    match status.speed {
                        Speed::Mbps10 => "10 Mbps",
                        Speed::Mbps100 => "100 Mbps",
                    },
                    match status.duplex {
                        Duplex::Half => "Half Duplex",
                        Duplex::Full => "Full Duplex",
                    }
                );

                // Configure MAC to match PHY link parameters
                critical_section::with(|cs| {
                    let mut emac_ref = EMAC.borrow_ref_mut(cs);
                    let emac = emac_ref.as_mut().expect("EMAC static unavailable");
                    emac.set_speed(status.speed);
                    emac.set_duplex(status.duplex);
                });
                link_up = true;
            }
            Ok(None) => {
                if attempt % 10 == 0 {
                    info!("Link down, waiting... (attempt {})", attempt);
                }
            }
            Err(e) => {
                warn!("Error polling link: {:?}", e);
            }
        }

        if link_up {
            break;
        }
    }

    if !link_up {
        error!("Timeout waiting for link");
        panic!("No Ethernet link");
    }

    // Start the EMAC
    info!("Starting EMAC...");
    critical_section::with(|cs| {
        if let Some(emac) = EMAC.borrow_ref_mut(cs).as_mut() {
            match emac.start() {
                Ok(()) => info!("EMAC started successfully"),
                Err(e) => {
                    error!("Failed to start EMAC: {:?}", e);
                    panic!("Cannot start EMAC");
                }
            }
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
        critical_section::with(|cs| {
            if let Some(emac) = EMAC.borrow_ref_mut(cs).as_mut() {
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
            }
        });

        // Print statistics every 10 seconds
        let now = time::Instant::now();
        if (now - last_stats_time).as_secs() >= 10 {
            info!("Stats: RX={}, Errors={}", frames_rx, errors);
            last_stats_time = now;

            // Also check link status periodically
            if let Ok(up) = phy.is_link_up(&mut mdio) {
                if !up {
                    warn!("Link is DOWN");
                }
            }
        }

        // Small delay to prevent tight polling
        delay.delay_micros(10);
    }
}
