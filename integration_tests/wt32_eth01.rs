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

/// Clock enable GPIO (controls external oscillator on WT32-ETH01)
const CLK_EN_GPIO: u8 = 16;

/// Reference clock input GPIO (for external clock mode)
const REF_CLK_GPIO: u8 = 0;

/// Reference clock output GPIO (for internal clock mode)
const REF_CLK_OUT_GPIO: u8 = 0;

/// Set to true for boards with external 50MHz oscillator (WT32-ETH01)
/// Set to false for plain ESP32 boards to test EMAC register access only
const USE_EXTERNAL_CLOCK: bool = true;

/// Set to true to skip full EMAC initialization (for boards without Ethernet hardware)
/// When true, only tests register access without DMA reset
const REGISTER_ACCESS_TEST_ONLY: bool = false;

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
    // Step 1: Clock source setup
    // -------------------------------------------------------------------------
    #[allow(unused_variables)]
    let clk_enable_pin: Option<Output<'_>>;

    if USE_EXTERNAL_CLOCK {
        // The WT32-ETH01 has an external crystal oscillator that must be enabled
        // by pulling GPIO16 HIGH. This oscillator provides the 50 MHz reference
        // clock to both the ESP32 EMAC and the LAN8720A PHY.
        info!("Enabling external oscillator (GPIO{})...", CLK_EN_GPIO);
        clk_enable_pin = Some(Output::new(
            peripherals.GPIO16,
            Level::High,
            OutputConfig::default(),
        ));

        // Wait for oscillator to stabilize (10ms is plenty)
        esp_hal::delay::Delay::new().delay_millis(10);
        info!("Oscillator enabled");
    } else {
        // For plain ESP32 boards without external Ethernet hardware,
        // use internal clock generation. The ESP32 will output the 50MHz
        // clock on the REF_CLK_OUT_GPIO pin.
        info!("Using internal clock generation (no external oscillator)");
        clk_enable_pin = None;
    }

    // -------------------------------------------------------------------------
    // Step 2: Test EMAC register access
    // -------------------------------------------------------------------------
    info!("Testing EMAC register access...");

    // Debug: Read DPORT WIFI_CLK_EN register to see current clock state
    let dport_wifi_clk = unsafe { core::ptr::read_volatile(0x3FF0_00CC as *const u32) };
    info!("DPORT WIFI_CLK_EN before init: {:#010x}", dport_wifi_clk);
    info!("  EMAC_EN (bit 14): {}", (dport_wifi_clk & (1 << 14)) != 0);

    // Enable EMAC peripheral clock if not already enabled
    if (dport_wifi_clk & (1 << 14)) == 0 {
        info!("Enabling EMAC peripheral clock via DPORT...");
        unsafe {
            let new_val = dport_wifi_clk | (1 << 14);
            core::ptr::write_volatile(0x3FF0_00CC as *mut u32, new_val);
        }
        let dport_after = unsafe { core::ptr::read_volatile(0x3FF0_00CC as *const u32) };
        info!("DPORT WIFI_CLK_EN after enable: {:#010x}", dport_after);
    }

    // Debug: Read DMA bus mode register to check if EMAC is accessible
    let dma_bus_mode = unsafe { core::ptr::read_volatile(0x3FF6_9000 as *const u32) };
    info!("DMA BUS_MODE: {:#010x}", dma_bus_mode);
    info!("  SW_RST (bit 0): {}", (dma_bus_mode & 1) != 0);

    // Read extension registers to verify access
    let ext_clkout = unsafe { core::ptr::read_volatile(0x3FF6_9800 as *const u32) };
    let ext_oscclk = unsafe { core::ptr::read_volatile(0x3FF6_9804 as *const u32) };
    let ext_clkctrl = unsafe { core::ptr::read_volatile(0x3FF6_9808 as *const u32) };
    let ext_phyinf = unsafe { core::ptr::read_volatile(0x3FF6_980C as *const u32) };

    info!("Extension registers:");
    info!("  EX_CLKOUT_CONF (0x800): {:#010x}", ext_clkout);
    info!("  EX_OSCCLK_CONF (0x804): {:#010x}", ext_oscclk);
    info!("  EX_CLK_CTRL    (0x808): {:#010x}", ext_clkctrl);
    info!("  EX_PHYINF_CONF (0x80C): {:#010x}", ext_phyinf);

    // Read MAC registers
    let mac_config = unsafe { core::ptr::read_volatile(0x3FF6_A000 as *const u32) };
    let mac_ff = unsafe { core::ptr::read_volatile(0x3FF6_A004 as *const u32) };
    info!("MAC registers:");
    info!("  GMACCONFIG (0x1000): {:#010x}", mac_config);
    info!("  GMACFF     (0x1004): {:#010x}", mac_ff);

    if REGISTER_ACCESS_TEST_ONLY {
        info!("");
        info!("=== REGISTER ACCESS TEST PASSED ===");
        info!("");
        info!("EMAC peripheral registers are accessible.");
        info!("DMA reset requires a working 50MHz reference clock which is not");
        info!("available on this board (no external oscillator, and internal");
        info!("clock generation requires APLL configuration).");
        info!("");
        info!("For full EMAC testing, use a board with Ethernet hardware like WT32-ETH01.");
        info!("");

        // Just loop forever
        loop {
            esp_hal::delay::Delay::new().delay_millis(1000);
        }
    }

    // -------------------------------------------------------------------------
    // Step 3: Full EMAC initialization (only if not in register-test-only mode)
    // -------------------------------------------------------------------------
    info!("Attempting full EMAC initialization...");

    // If SW_RST is already set, we may need to wait for it to clear or
    // the EMAC needs a full power cycle
    if (dma_bus_mode & 1) != 0 {
        warn!("SW_RST bit is already set! Waiting for it to clear...");
        let mut delay = esp_hal::delay::Delay::new();
        for i in 0..100 {
            delay.delay_millis(10);
            let bus_mode = unsafe { core::ptr::read_volatile(0x3FF6_9000 as *const u32) };
            if (bus_mode & 1) == 0 {
                info!("SW_RST cleared after {} ms", (i + 1) * 10);
                break;
            }
            if i == 99 {
                warn!("SW_RST still set after 1s, trying to clear manually...");
                // Try writing 0 to clear
                unsafe { core::ptr::write_volatile(0x3FF6_9000 as *mut u32, 0) };
                delay.delay_millis(10);
                let bus_mode = unsafe { core::ptr::read_volatile(0x3FF6_9000 as *const u32) };
                info!("DMA BUS_MODE after manual clear: {:#010x}", bus_mode);
            }
        }
    }

    // Note: MDC (GPIO23) and MDIO (GPIO18) are fixed by ESP32 hardware
    // The RMII data pins are also fixed (see board config for reference)
    let rmii_clock_mode = if USE_EXTERNAL_CLOCK {
        info!(
            "Configuring RMII with external clock input on GPIO{}",
            REF_CLK_GPIO
        );
        RmiiClockMode::ExternalInput { gpio: REF_CLK_GPIO }
    } else {
        info!(
            "Configuring RMII with internal clock output on GPIO{}",
            REF_CLK_OUT_GPIO
        );
        RmiiClockMode::InternalOutput {
            gpio: REF_CLK_OUT_GPIO,
        }
    };

    let config = EmacConfig::new()
        .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56]) // Locally administered
        .with_phy_interface(PhyInterface::Rmii)
        .with_rmii_clock(rmii_clock_mode);

    // IMPORTANT: We must place the EMAC in its final static location BEFORE
    // calling init(), because init() sets up DMA descriptor chains that contain
    // pointers to themselves. If we init() on the stack and then move, the
    // pointers will be invalid!
    
    // First, place an uninitialized EMAC in the static
    critical_section::with(|cs| {
        EMAC.borrow_ref_mut(cs).replace(Emac::new());
    });
    
    // Now initialize it in-place through the static reference
    critical_section::with(|cs| {
        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
            let mut delay = esp_hal::delay::Delay::new();
            match emac.init(config, &mut delay) {
                Ok(()) => {
                    info!("EMAC initialized successfully");
                }
                Err(e) => {
                    error!("EMAC init failed: {:?}", e);
                    panic!("Cannot continue without EMAC");
                }
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
    // Step 6: Main loop - Packet reception test
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
                                    dst_mac[0],
                                    dst_mac[1],
                                    dst_mac[2],
                                    dst_mac[3],
                                    dst_mac[4],
                                    dst_mac[5]
                                );
                                info!(
                                    "  Src: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                                    src_mac[0],
                                    src_mac[1],
                                    src_mac[2],
                                    src_mac[3],
                                    src_mac[4],
                                    src_mac[5]
                                );

                                // Check for common EtherTypes
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
                                    0x88CC => info!("  Type: LLDP"),
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

            // Re-check link status using is_link_up() for current state
            // Note: poll_link() only reports transitions, not current state
            match phy.is_link_up(&mut mdio) {
                Ok(true) => info!("Link: UP"),
                Ok(false) => warn!("Link: DOWN"),
                Err(e) => error!("Link check error: {:?}", e),
            }
        }

        // Small delay to avoid tight spinning
        esp_hal::delay::Delay::new().delay_micros(100);
    }
}
