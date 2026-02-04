//! Integration Test Binary for WT32-ETH01 Board
//!
//! This binary tests the ph-esp32-mac driver with the popular WT32-ETH01
//! development board, which features:
//!
//! - ESP32 (WT32-S1 module, ESP32-WROOM-32E compatible)
//! - LAN8720A Ethernet PHY
//! - External 50 MHz crystal oscillator (enabled via GPIO16)
//! - RJ45 jack with integrated magnetics
//!
//! # Hardware Configuration (WT32-ETH01 specific)
//!
//! | Function    | GPIO | Notes                                    |
//! |-------------|------|------------------------------------------|
//! | MDC         | 23   | MDIO clock (SMI)                         |
//! | MDIO        | 18   | MDIO data (SMI)                          |
//! | TX_EN       | 21   | Transmit enable                          |
//! | TXD0        | 19   | Transmit data 0                          |
//! | TXD1        | 22   | Transmit data 1                          |
//! | CRS_DV      | 27   | Carrier sense / RX data valid            |
//! | RXD0        | 25   | Receive data 0                           |
//! | RXD1        | 26   | Receive data 1                           |
//! | REF_CLK     | 0    | 50 MHz reference clock input             |
//! | CLK_EN      | 16   | Clock enable (pull HIGH to enable osc)   |
//! | PHY_ADDR    | -    | 1 (PHYAD0 pulled high on WT32-ETH01)     |
//!
//! # Programming
//!
//! The WT32-ETH01 requires an external USB-TTL programmer. Connect:
//! - 3.3V -> 3V3
//! - GND  -> GND  
//! - TX   -> RXD (IO3)
//! - RX   -> TXD (IO1)
//! - IO0 must be LOW during reset to enter bootloader mode
//!
//! # Building and Flashing
//!
//! ```bash
//! cd integration_tests
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use core::cell::RefCell;

use critical_section::Mutex;
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    main,
};
use log::{error, info, warn};

// ESP-IDF bootloader requires an app descriptor
esp_app_desc!();

use ph_esp32_mac::{
    Duplex, Emac, EmacConfig, Lan8720a, MdioController, PhyDriver, PhyInterface, RmiiClockMode,
    Speed,
};

// =============================================================================
// WT32-ETH01 Board Configuration
// =============================================================================

/// WT32-ETH01 PHY address (PHYAD0 is pulled high on this board)
const PHY_ADDR: u8 = 1;

/// Clock enable GPIO (controls external oscillator)
const CLK_EN_GPIO: u8 = 16;

/// Reference clock input GPIO
const REF_CLK_GPIO: u8 = 0;

// =============================================================================
// Static EMAC Instance
// =============================================================================

/// Static EMAC instance with 10 RX/TX buffers and 1600 byte frames
///
/// This consumes approximately 32KB of SRAM in the DMA-capable region.
static EMAC: Mutex<RefCell<Option<Emac<10, 10, 1600>>>> = Mutex::new(RefCell::new(None));

// =============================================================================
// Main Entry Point
// =============================================================================

#[main]
fn main() -> ! {
    // Initialize logging
    esp_println::logger::init_logger_from_env();
    info!("WT32-ETH01 Ethernet Integration Test");
    info!("=====================================");

    // Initialize ESP-HAL peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // -------------------------------------------------------------------------
    // Step 1: Enable the external 50 MHz oscillator
    // -------------------------------------------------------------------------
    // The WT32-ETH01 has an external crystal oscillator that must be enabled
    // by pulling GPIO16 HIGH. This oscillator provides the 50 MHz reference
    // clock to both the ESP32 EMAC and the LAN8720A PHY.
    info!("Enabling external oscillator (GPIO{})...", CLK_EN_GPIO);
    let _clk_enable = Output::new(peripherals.GPIO16, Level::High, OutputConfig::default());
    
    // Wait for oscillator to stabilize (10ms is plenty)
    esp_hal::delay::Delay::new().delay_millis(10);
    info!("Oscillator enabled");

    // -------------------------------------------------------------------------
    // Step 2: Configure and initialize the EMAC
    // -------------------------------------------------------------------------
    info!("Initializing EMAC...");

    // Note: MDC (GPIO23) and MDIO (GPIO18) are fixed by ESP32 hardware
    // The RMII data pins are also fixed (see board config for reference)
    let config = EmacConfig::new()
        .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56]) // Locally administered
        .with_phy_interface(PhyInterface::Rmii)
        .with_rmii_clock(RmiiClockMode::ExternalInput { gpio: REF_CLK_GPIO });

    // Initialize EMAC in critical section
    critical_section::with(|cs| {
        let mut emac = Emac::new();
        let mut delay = esp_hal::delay::Delay::new();
        
        match emac.init(config, &mut delay) {
            Ok(()) => {
                info!("EMAC initialized successfully");
                EMAC.borrow_ref_mut(cs).replace(emac);
            }
            Err(e) => {
                error!("EMAC init failed: {:?}", e);
                panic!("Cannot continue without EMAC");
            }
        }
    });

    // -------------------------------------------------------------------------
    // Step 3: Initialize the LAN8720A PHY
    // -------------------------------------------------------------------------
    info!("Initializing LAN8720A PHY at address {}...", PHY_ADDR);

    let mut phy = Lan8720a::new(PHY_ADDR);

    // Create MDIO controller for PHY communication
    let mut mdio = MdioController::new(esp_hal::delay::Delay::new());

    match phy.init(&mut mdio) {
        Ok(()) => info!("PHY initialized successfully"),
        Err(e) => {
            error!("PHY init failed: {:?}", e);
            panic!("Cannot continue without PHY");
        }
    }

    // Read PHY ID to verify communication
    if let Ok(phy_id) = phy.phy_id(&mut mdio) {
        info!("PHY ID: 0x{:08X}", phy_id);
        if phy_id & 0xFFFF_FFF0 == 0x0007_C0F0 {
            info!("  -> Confirmed: LAN8720A/LAN8720AI");
        } else {
            warn!("  -> Unexpected PHY ID (expected LAN8720A)");
        }
    }

    // -------------------------------------------------------------------------
    // Step 4: Wait for link and configure MAC
    // -------------------------------------------------------------------------
    info!("Waiting for Ethernet link...");
    
    let mut link_up = false;
    let mut retry_count = 0;
    const MAX_RETRIES: u32 = 100; // 10 seconds

    while !link_up && retry_count < MAX_RETRIES {
        match phy.poll_link(&mut mdio) {
            Ok(Some(link_status)) => {
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

                // Configure MAC with negotiated link parameters
                critical_section::with(|cs| {
                    if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                        emac.set_speed(link_status.speed);
                        emac.set_duplex(link_status.duplex);
                    }
                });

                link_up = true;
            }
            Ok(None) => {
                // No link yet
                retry_count += 1;
                if retry_count % 10 == 0 {
                    info!("  Still waiting... ({}/{})", retry_count, MAX_RETRIES);
                }
                esp_hal::delay::Delay::new().delay_millis(100);
            }
            Err(e) => {
                error!("Link poll error: {:?}", e);
                esp_hal::delay::Delay::new().delay_millis(100);
            }
        }
    }

    if !link_up {
        error!("Timeout waiting for link!");
        error!("Check cable connection and network equipment.");
        panic!("No Ethernet link");
    }

    // -------------------------------------------------------------------------
    // Step 5: Start EMAC
    // -------------------------------------------------------------------------
    info!("Starting EMAC...");

    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            match emac.start() {
                Ok(()) => info!("EMAC started successfully"),
                Err(e) => {
                    error!("EMAC start failed: {:?}", e);
                    panic!("Cannot start EMAC");
                }
            }
        }
    });

    // -------------------------------------------------------------------------
    // Step 6: Main loop - Echo/ping responder test
    // -------------------------------------------------------------------------
    info!("");
    info!("=== INTEGRATION TEST ACTIVE ===");
    info!("EMAC is running. Testing packet reception...");
    info!("");

    let mut rx_buffer = [0u8; 1600];
    let mut packet_count: u32 = 0;
    let mut last_status_time: u32 = 0;

    loop {
        // Check for received packets
        critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                if emac.rx_available() {
                    match emac.receive(&mut rx_buffer) {
                        Ok(len) => {
                            packet_count += 1;
                            
                            // Parse Ethernet header
                            if len >= 14 {
                                let dst_mac = &rx_buffer[0..6];
                                let src_mac = &rx_buffer[6..12];
                                let ethertype = u16::from_be_bytes([rx_buffer[12], rx_buffer[13]]);
                                
                                info!(
                                    "RX #{}: {} bytes, EtherType=0x{:04X}",
                                    packet_count, len, ethertype
                                );
                                info!(
                                    "  Dst: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                                    dst_mac[0], dst_mac[1], dst_mac[2],
                                    dst_mac[3], dst_mac[4], dst_mac[5]
                                );
                                info!(
                                    "  Src: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                                    src_mac[0], src_mac[1], src_mac[2],
                                    src_mac[3], src_mac[4], src_mac[5]
                                );

                                // Check for ARP (0x0806) or IPv4 (0x0800)
                                match ethertype {
                                    0x0806 => info!("  Type: ARP"),
                                    0x0800 => {
                                        if len >= 34 {
                                            let protocol = rx_buffer[23];
                                            match protocol {
                                                1 => info!("  Type: IPv4/ICMP"),
                                                6 => info!("  Type: IPv4/TCP"),
                                                17 => info!("  Type: IPv4/UDP"),
                                                _ => info!("  Type: IPv4/proto={}", protocol),
                                            }
                                        }
                                    }
                                    0x86DD => info!("  Type: IPv6"),
                                    _ => {}
                                }
                            }
                        }
                        Err(e) => {
                            error!("RX error: {:?}", e);
                        }
                    }
                }
            }
        });

        // Periodic status report (every ~5 seconds)
        last_status_time += 1;
        if last_status_time >= 5000 {
            last_status_time = 0;
            info!("--- Status: {} packets received ---", packet_count);
            
            // Re-check link status
            match phy.poll_link(&mut mdio) {
                Ok(Some(_)) => info!("Link: UP"),
                Ok(None) => warn!("Link: DOWN"),
                Err(e) => error!("Link check error: {:?}", e),
            }
        }

        // Small delay to avoid tight spinning
        esp_hal::delay::Delay::new().delay_micros(100);
    }
}
