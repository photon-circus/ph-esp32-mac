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
//! cargo build --example esp_hal_example --target xtensa-esp32-none-elf --release
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
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main, time,
};
use log::{error, info, warn};

use ph_esp32_mac::{
    Duplex, Emac, EmacConfig, InterruptStatus, Lan8720a, MdioController, PhyDriver, PhyInterface,
    RmiiClockMode, Speed,
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
    info!("External oscillator enabled (GPIO16 = HIGH)");

    // Small delay for oscillator startup
    delay.delay_millis(10);

    // Initialize EMAC
    critical_section::with(|cs| {
        let mut emac = Emac::new();

        // Configure for WT32-ETH01 board
        let config = EmacConfig::new()
            .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
            .with_phy_interface(PhyInterface::Rmii)
            .with_rmii_clock(RmiiClockMode::ExternalGpio0);

        info!("Initializing EMAC...");
        match emac.init(config, &mut delay) {
            Ok(()) => info!("EMAC initialized successfully"),
            Err(e) => {
                error!("EMAC initialization failed: {:?}", e);
                panic!("Cannot continue without EMAC");
            }
        }

        EMAC.borrow_ref_mut(cs).replace(emac);
    });

    // Initialize PHY
    info!("Initializing LAN8720A PHY at address {}...", PHY_ADDR);

    // Create MDIO controller with esp-hal delay
    let mut phy = Lan8720a::new(PHY_ADDR);

    critical_section::with(|cs| {
        if let Some(emac) = EMAC.borrow_ref_mut(cs).as_mut() {
            // Use MdioController for PHY communication
            let mut mdio = MdioController::new(&mut delay);

            match phy.init(&mut mdio) {
                Ok(()) => info!("PHY initialized successfully"),
                Err(e) => {
                    error!("PHY initialization failed: {:?}", e);
                    panic!("Cannot continue without PHY");
                }
            }

            // Check PHY ID
            match phy.read_id(&mut mdio) {
                Ok(id) => info!("PHY ID: 0x{:08X}", id),
                Err(e) => warn!("Could not read PHY ID: {:?}", e),
            }
        }
    });

    // Wait for link to come up
    info!("Waiting for Ethernet link...");
    let mut link_up = false;

    for attempt in 0..50 {
        delay.delay_millis(200);

        critical_section::with(|cs| {
            if let Some(emac) = EMAC.borrow_ref_mut(cs).as_mut() {
                let mut mdio = MdioController::new(&mut delay);

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
                        emac.set_speed(status.speed);
                        emac.set_duplex(status.duplex);
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
            }
        });

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

    info!("Ethernet is ready! Running echo server...");
    info!(
        "Memory usage: {} bytes",
        Emac::<10, 10, 1600>::memory_usage()
    );

    // Main loop: simple frame echo with statistics
    let mut rx_buffer = [0u8; 1600];
    let mut frames_rx = 0u32;
    let mut frames_tx = 0u32;
    let mut errors = 0u32;
    let mut last_stats_time = time::Instant::now();

    loop {
        critical_section::with(|cs| {
            if let Some(emac) = EMAC.borrow_ref_mut(cs).as_mut() {
                // Try to receive a frame
                match emac.receive(&mut rx_buffer) {
                    Ok(len) if len > 0 => {
                        frames_rx += 1;

                        // Log frame info (first frame only to avoid spam)
                        if frames_rx == 1 {
                            info!("First frame received: {} bytes", len);
                            info!(
                                "  Dst: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                                rx_buffer[0],
                                rx_buffer[1],
                                rx_buffer[2],
                                rx_buffer[3],
                                rx_buffer[4],
                                rx_buffer[5]
                            );
                            info!(
                                "  Src: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                                rx_buffer[6],
                                rx_buffer[7],
                                rx_buffer[8],
                                rx_buffer[9],
                                rx_buffer[10],
                                rx_buffer[11]
                            );
                        }

                        // Echo: swap MAC addresses and send back
                        let frame = &mut rx_buffer[..len];
                        for i in 0..6 {
                            frame.swap(i, i + 6);
                        }

                        match emac.transmit(&frame[..len]) {
                            Ok(_) => frames_tx += 1,
                            Err(_) => errors += 1,
                        }
                    }
                    Err(_) => errors += 1,
                    _ => {}
                }
            }
        });

        // Print statistics every 10 seconds
        let now = time::Instant::now();
        if now.duration_since(last_stats_time).as_secs() >= 10 {
            info!(
                "Stats: RX={}, TX={}, Errors={}",
                frames_rx, frames_tx, errors
            );
            last_stats_time = now;

            // Also check link status periodically
            critical_section::with(|cs| {
                if EMAC.borrow_ref(cs).is_some() {
                    let mut mdio = MdioController::new(&mut delay);

                    if let Ok(up) = phy.is_link_up(&mut mdio) {
                        if !up {
                            warn!("Link is DOWN");
                        }
                    }
                }
            });
        }

        // Small delay to prevent tight polling
        delay.delay_micros(10);
    }
}
