//! Integration Test Binary for WT32-ETH01 Board
//!
//! This binary runs a series of hardware integration tests on the WT32-ETH01
//! development board to verify the ph-esp32-mac driver functionality.
//!
//! # Test Categories
//!
//! 1. **Register Access Tests** - Verify EMAC peripheral registers are accessible
//! 2. **PHY Tests** - Verify LAN8720A PHY communication and link detection
//! 3. **DMA Tests** - Verify descriptor chain setup and buffer management
//! 4. **RX Tests** - Verify packet reception works correctly
//! 5. **TX Tests** - Verify packet transmission works correctly
//! 6. **smoltcp Tests** - Verify network stack integration
//!
//! # Building and Flashing
//!
//! ```bash
//! cd integration_tests
//! cargo run --release
//! ```

#![no_std]
#![no_main]

mod boards;

use core::cell::RefCell;

use critical_section::Mutex;
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    main,
};
use log::{error, info, warn};

esp_app_desc!();

use ph_esp32_mac::{
    Duplex, Emac, EmacConfig, Lan8720a, MdioController, PhyDriver, PhyInterface, RmiiClockMode,
    Speed,
};

// smoltcp imports
use smoltcp::iface::{Config as IfaceConfig, Interface, PollResult, SocketSet};
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, Ipv4Address};
use smoltcp::time::Instant;

use boards::wt32_eth01::Wt32Eth01Config as Board;

// =============================================================================
// Test Framework
// =============================================================================

/// Test result type
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TestResult {
    Pass,
    Fail,
    Skip,
}

impl TestResult {
    fn symbol(&self) -> &'static str {
        match self {
            TestResult::Pass => "✓",
            TestResult::Fail => "✗",
            TestResult::Skip => "○",
        }
    }
}

/// Accumulated test statistics
struct TestStats {
    passed: u32,
    failed: u32,
    skipped: u32,
}

impl TestStats {
    const fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
            skipped: 0,
        }
    }

    fn record(&mut self, result: TestResult) {
        match result {
            TestResult::Pass => self.passed += 1,
            TestResult::Fail => self.failed += 1,
            TestResult::Skip => self.skipped += 1,
        }
    }

    fn total(&self) -> u32 {
        self.passed + self.failed + self.skipped
    }

    fn all_passed(&self) -> bool {
        self.failed == 0
    }
}

/// Run a single test, log the result, and record statistics
macro_rules! run_test {
    ($stats:expr, $name:expr, $test_fn:expr) => {{
        info!("");
        info!("▶ {}", $name);
        let result = $test_fn;
        match result {
            TestResult::Pass => info!("  {} PASS", result.symbol()),
            TestResult::Fail => error!("  {} FAIL", result.symbol()),
            TestResult::Skip => warn!("  {} SKIP", result.symbol()),
        }
        $stats.record(result);
        result
    }};
}

// =============================================================================
// Hardware Constants
// =============================================================================

const IO_MUX_BASE: u32 = 0x3FF4_9000;
const DMA_BASE: u32 = 0x3FF6_9000;
const MAC_BASE: u32 = 0x3FF6_A000;
const DPORT_WIFI_CLK_EN: u32 = 0x3FF0_00CC;

// =============================================================================
// Static EMAC Instance
// =============================================================================

/// Static EMAC instance with 4 RX/TX buffers and 1600 byte frames
/// 
/// We use 4 buffers for minimal memory footprint during tests.
/// Production code typically uses 8-16 buffers.
static EMAC: Mutex<RefCell<Option<Emac<4, 4, 1600>>>> = Mutex::new(RefCell::new(None));

// =============================================================================
// Test Context - Shared state across tests
// =============================================================================

struct TestContext<'a> {
    phy: Lan8720a,
    mdio: MdioController<esp_hal::delay::Delay>,
    #[allow(dead_code)]
    clk_pin: Option<Output<'a>>,
    link_speed: Speed,
    link_duplex: Duplex,
    emac_initialized: bool,
    link_up: bool,
}

impl<'a> TestContext<'a> {
    fn new(clk_pin: Output<'a>) -> Self {
        Self {
            phy: Lan8720a::new(Board::PHY_ADDR),
            mdio: MdioController::new(esp_hal::delay::Delay::new()),
            clk_pin: Some(clk_pin),
            link_speed: Speed::Mbps100,
            link_duplex: Duplex::Full,
            emac_initialized: false,
            link_up: false,
        }
    }
}

// =============================================================================
// Test Group 1: Register Access
// =============================================================================

mod register_tests {
    use super::*;

    /// Verify EMAC peripheral clock can be enabled via DPORT
    pub fn test_emac_clock_enable() -> TestResult {
        let clk_reg = unsafe { core::ptr::read_volatile(DPORT_WIFI_CLK_EN as *const u32) };
        
        if (clk_reg & (1 << 14)) != 0 {
            info!("  DPORT WIFI_CLK_EN={:#010x}, EMAC_EN=1", clk_reg);
            return TestResult::Pass;
        }
        
        // Enable EMAC clock
        info!("  Enabling EMAC clock...");
        unsafe {
            core::ptr::write_volatile(DPORT_WIFI_CLK_EN as *mut u32, clk_reg | (1 << 14));
        }
        
        let clk_after = unsafe { core::ptr::read_volatile(DPORT_WIFI_CLK_EN as *const u32) };
        if (clk_after & (1 << 14)) != 0 {
            info!("  EMAC clock enabled successfully");
            TestResult::Pass
        } else {
            error!("  Failed to enable EMAC clock");
            TestResult::Fail
        }
    }

    /// Verify DMA registers are readable and contain reasonable values
    pub fn test_dma_registers_accessible() -> TestResult {
        let bus_mode = unsafe { core::ptr::read_volatile(DMA_BASE as *const u32) };
        info!("  DMA BUS_MODE={:#010x}", bus_mode);
        
        if bus_mode == 0x0000_0000 || bus_mode == 0xFFFF_FFFF {
            error!("  DMA registers not accessible");
            TestResult::Fail
        } else {
            TestResult::Pass
        }
    }

    /// Verify MAC registers are readable
    pub fn test_mac_registers_accessible() -> TestResult {
        let mac_config = unsafe { core::ptr::read_volatile(MAC_BASE as *const u32) };
        let mac_ff = unsafe { core::ptr::read_volatile((MAC_BASE + 4) as *const u32) };
        
        info!("  GMACCONFIG={:#010x}, GMACFF={:#010x}", mac_config, mac_ff);
        
        if mac_config == 0xFFFF_FFFF || mac_ff == 0xFFFF_FFFF {
            error!("  MAC registers not accessible");
            TestResult::Fail
        } else {
            TestResult::Pass
        }
    }

    /// Verify extension registers are readable  
    pub fn test_extension_registers() -> TestResult {
        let ext_clkout = unsafe { core::ptr::read_volatile((DMA_BASE + 0x800) as *const u32) };
        let ext_phyinf = unsafe { core::ptr::read_volatile((DMA_BASE + 0x80C) as *const u32) };
        
        info!("  EX_CLKOUT_CONF={:#010x}, EX_PHYINF_CONF={:#010x}", ext_clkout, ext_phyinf);
        
        if ext_clkout == 0xFFFF_FFFF || ext_phyinf == 0xFFFF_FFFF {
            error!("  Extension registers not accessible");
            TestResult::Fail
        } else {
            TestResult::Pass
        }
    }
}

// =============================================================================
// Test Group 2: EMAC Initialization
// =============================================================================

mod init_tests {
    use super::*;

    /// Test EMAC initialization with board-specific configuration
    pub fn test_emac_init(ctx: &mut TestContext) -> TestResult {
        let config = EmacConfig::new()
            .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
            .with_phy_interface(PhyInterface::Rmii)
            .with_rmii_clock(RmiiClockMode::ExternalInput { 
                gpio: Board::REF_CLK_GPIO 
            });

        // Place EMAC in static location BEFORE init (required for DMA descriptors)
        critical_section::with(|cs| {
            EMAC.borrow_ref_mut(cs).replace(Emac::new());
        });

        // Initialize in-place
        let result = critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                let mut delay = esp_hal::delay::Delay::new();
                match emac.init(config, &mut delay) {
                    Ok(()) => {
                        info!("  EMAC initialized");
                        ctx.emac_initialized = true;
                        TestResult::Pass
                    }
                    Err(e) => {
                        error!("  EMAC init failed: {:?}", e);
                        TestResult::Fail
                    }
                }
            } else {
                error!("  EMAC static unavailable");
                TestResult::Fail
            }
        });

        result
    }

    /// Verify RMII pins are correctly configured via IO_MUX
    pub fn test_rmii_pins() -> TestResult {
        let pins = [
            ("GPIO0  REF_CLK", 0x44u32, true),
            ("GPIO19 TXD0", 0x74, false),
            ("GPIO21 TX_EN", 0x7C, false),
            ("GPIO22 TXD1", 0x80, false),
            ("GPIO25 RXD0", 0x24, true),
            ("GPIO26 RXD1", 0x28, true),
            ("GPIO27 CRS_DV", 0x2C, true),
        ];

        let mut all_ok = true;

        for (name, offset, is_input) in pins {
            let reg = unsafe { core::ptr::read_volatile((IO_MUX_BASE + offset) as *const u32) };
            let mcu_sel = (reg >> 12) & 0x7;
            let fun_ie = (reg >> 9) & 0x1;
            
            let ok = mcu_sel == 5 && (!is_input || fun_ie == 1);
            
            if ok {
                info!("  {} MCU_SEL=5 {}", name, if is_input { "FUN_IE=1" } else { "" });
            } else {
                error!("  {} MCU_SEL={} FUN_IE={} EXPECTED MCU_SEL=5", name, mcu_sel, fun_ie);
                all_ok = false;
            }
        }

        if all_ok { TestResult::Pass } else { TestResult::Fail }
    }

    /// Verify DMA descriptor chain is correctly linked
    pub fn test_dma_descriptor_chain() -> TestResult {
        let rx_base = unsafe { core::ptr::read_volatile((DMA_BASE + 0x0C) as *const u32) };
        let tx_base = unsafe { core::ptr::read_volatile((DMA_BASE + 0x10) as *const u32) };
        
        // Split address printing to avoid espflash addr2line symbolication
        info!("  RX desc base=0x{:04X}_{:04X}, TX desc base=0x{:04X}_{:04X}", 
              (rx_base >> 16) & 0xFFFF, rx_base & 0xFFFF,
              (tx_base >> 16) & 0xFFFF, tx_base & 0xFFFF);
        
        // Verify addresses are in SRAM region
        let sram_ok = |addr: u32| addr >= 0x3FFB_0000 && addr < 0x4000_0000;
        
        if !sram_ok(rx_base) || !sram_ok(tx_base) {
            error!("  Descriptor bases not in SRAM region");
            return TestResult::Fail;
        }
        
        // Check descriptor chain linkage (4 descriptors, 32 bytes each)
        for i in 0..4u32 {
            let desc_addr = rx_base + (i * 32);
            let rdes0 = unsafe { core::ptr::read_volatile(desc_addr as *const u32) };
            let rdes3 = unsafe { core::ptr::read_volatile((desc_addr + 12) as *const u32) };
            
            let expected_next = if i == 3 { rx_base } else { rx_base + ((i + 1) * 32) };
            let dma_owns = (rdes0 & 0x8000_0000) != 0;
            
            if rdes3 != expected_next || !dma_owns {
                error!("  Desc[{}] NEXT=0x{:04X}_{:04X} expected 0x{:04X}_{:04X}, OWN={}", 
                       i, 
                       (rdes3 >> 16) & 0xFFFF, rdes3 & 0xFFFF,
                       (expected_next >> 16) & 0xFFFF, expected_next & 0xFFFF,
                       if dma_owns { 1 } else { 0 });
                return TestResult::Fail;
            }
        }
        
        info!("  Descriptor chain valid and loops correctly");
        TestResult::Pass
    }
}

// =============================================================================
// Test Group 3: PHY Communication
// =============================================================================

mod phy_tests {
    use super::*;

    /// Verify PHY responds to MDIO read operations
    pub fn test_phy_mdio_read(ctx: &mut TestContext) -> TestResult {
        match ctx.phy.phy_id(&mut ctx.mdio) {
            Ok(phy_id) => {
                info!("  PHY ID={:#010x}", phy_id);
                
                if phy_id & Board::PHY_ID_MASK == Board::PHY_ID {
                    info!("  Confirmed: {}", Board::PHY_TYPE);
                } else {
                    warn!("  Unexpected PHY ID (expected {})", Board::PHY_TYPE);
                }
                TestResult::Pass
            }
            Err(e) => {
                error!("  MDIO read failed: {:?}", e);
                TestResult::Fail
            }
        }
    }

    /// Test PHY initialization
    pub fn test_phy_init(ctx: &mut TestContext) -> TestResult {
        match ctx.phy.init(&mut ctx.mdio) {
            Ok(()) => {
                info!("  PHY initialized");
                TestResult::Pass
            }
            Err(e) => {
                error!("  PHY init failed: {:?}", e);
                TestResult::Fail
            }
        }
    }

    /// Test PHY link detection (requires cable connected)
    pub fn test_phy_link_up(ctx: &mut TestContext, timeout_ms: u32) -> TestResult {
        info!("  Waiting for link (max {}ms)...", timeout_ms);
        
        let delay = esp_hal::delay::Delay::new();
        let iterations = timeout_ms / 100;
        
        for i in 0..iterations {
            match ctx.phy.poll_link(&mut ctx.mdio) {
                Ok(Some(status)) => {
                    ctx.link_speed = status.speed;
                    ctx.link_duplex = status.duplex;
                    ctx.link_up = true;
                    
                    let speed = match status.speed { 
                        Speed::Mbps10 => "10Mbps", 
                        Speed::Mbps100 => "100Mbps" 
                    };
                    let duplex = match status.duplex { 
                        Duplex::Half => "Half", 
                        Duplex::Full => "Full" 
                    };
                    
                    info!("  Link UP: {} {} ({}ms)", speed, duplex, (i + 1) * 100);
                    
                    // Configure MAC with negotiated parameters
                    critical_section::with(|cs| {
                        if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                            emac.set_speed(status.speed);
                            emac.set_duplex(status.duplex);
                        }
                    });
                    
                    return TestResult::Pass;
                }
                Ok(None) => {
                    delay.delay_millis(100);
                }
                Err(e) => {
                    error!("  Link poll error: {:?}", e);
                    return TestResult::Fail;
                }
            }
        }
        
        warn!("  Link timeout - is cable connected?");
        TestResult::Skip
    }
}

// =============================================================================
// Test Group 4: EMAC Operations
// =============================================================================

mod emac_tests {
    use super::*;

    /// Test EMAC can be started
    pub fn test_emac_start() -> TestResult {
        critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                match emac.start() {
                    Ok(()) => {
                        info!("  EMAC started");
                        TestResult::Pass
                    }
                    Err(e) => {
                        error!("  EMAC start failed: {:?}", e);
                        TestResult::Fail
                    }
                }
            } else {
                error!("  EMAC not available");
                TestResult::Fail
            }
        })
    }

    /// Test packet transmission
    pub fn test_packet_tx() -> TestResult {
        // Build a broadcast test frame
        let mut frame = [0u8; 64];
        frame[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Dst: broadcast
        frame[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x12, 0x34, 0x56]); // Src: our MAC
        frame[12..14].copy_from_slice(&[0x88, 0xB5]); // EtherType: local experimental
        for i in 14..64 {
            frame[i] = (i - 14) as u8; // Payload: incrementing pattern
        }
        
        let result = critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                match emac.transmit(&frame) {
                    Ok(_len) => {
                        info!("  Transmitted 64-byte broadcast frame");
                        TestResult::Pass
                    }
                    Err(e) => {
                        error!("  TX failed: {:?}", e);
                        TestResult::Fail
                    }
                }
            } else {
                error!("  EMAC not available");
                TestResult::Fail
            }
        });
        
        esp_hal::delay::Delay::new().delay_millis(10);
        result
    }

    /// Test packet reception
    pub fn test_packet_rx(duration_secs: u32) -> TestResult {
        info!("  Listening for {} seconds...", duration_secs);
        
        let mut rx_buffer = [0u8; 1600];
        let mut packet_count = 0u32;
        let delay = esp_hal::delay::Delay::new();
        let iterations = duration_secs * 1000;
        
        for _ in 0..iterations {
            critical_section::with(|cs| {
                if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                    while emac.rx_available() {
                        if let Ok(len) = emac.receive(&mut rx_buffer) {
                            packet_count += 1;
                            if packet_count <= 3 && len >= 14 {
                                let etype = u16::from_be_bytes([rx_buffer[12], rx_buffer[13]]);
                                info!("    Packet #{}: {} bytes, EtherType=0x{:04X}", 
                                      packet_count, len, etype);
                            }
                        }
                    }
                }
            });
            delay.delay_millis(1);
        }
        
        info!("  Received {} packets", packet_count);
        
        if packet_count > 0 {
            TestResult::Pass
        } else {
            warn!("  No packets received");
            TestResult::Skip
        }
    }

    /// Test EMAC can be stopped and restarted
    pub fn test_emac_stop_start() -> TestResult {
        let stop_result = critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                let _ = emac.stop();
                true
            } else {
                false
            }
        });
        
        if !stop_result {
            error!("  EMAC not available");
            return TestResult::Fail;
        }
        
        info!("  EMAC stopped");
        esp_hal::delay::Delay::new().delay_millis(100);
        
        let start_result = critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                emac.start()
            } else {
                Err(ph_esp32_mac::Error::Config(ph_esp32_mac::ConfigError::InvalidConfig))
            }
        });
        
        match start_result {
            Ok(()) => {
                info!("  EMAC restarted");
                TestResult::Pass
            }
            Err(e) => {
                error!("  EMAC restart failed: {:?}", e);
                TestResult::Fail
            }
        }
    }
}

// =============================================================================
// Test Group 5: Link Status
// =============================================================================

mod link_tests {
    use super::*;

    /// Test link status query
    pub fn test_link_status_query(ctx: &mut TestContext) -> TestResult {
        match ctx.phy.is_link_up(&mut ctx.mdio) {
            Ok(true) => {
                info!("  Link: UP");
                TestResult::Pass
            }
            Ok(false) => {
                warn!("  Link: DOWN");
                TestResult::Skip
            }
            Err(e) => {
                error!("  Link query failed: {:?}", e);
                TestResult::Fail
            }
        }
    }
}

// =============================================================================
// Test Group 6: smoltcp Integration
// =============================================================================

mod smoltcp_tests {
    use super::*;

    /// Our IP configuration for testing
    const OUR_IP: Ipv4Address = Ipv4Address::new(192, 168, 1, 200);
    #[allow(dead_code)]
    const GATEWAY_IP: Ipv4Address = Ipv4Address::new(192, 168, 1, 1);

    /// Test smoltcp interface creation
    pub fn test_interface_creation() -> TestResult {
        info!("  Creating smoltcp interface...");
        
        // Get MAC address from EMAC
        let mac = critical_section::with(|cs| {
            if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
                Some(*emac.mac_address())
            } else {
                None
            }
        });

        let Some(mac) = mac else {
            error!("  EMAC not available");
            return TestResult::Fail;
        };

        info!("  MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
        info!("  IP:  192.168.1.200/24");
        
        // Test that we can construct an interface config
        let ethernet_addr = EthernetAddress(mac);
        let _config = IfaceConfig::new(HardwareAddress::Ethernet(ethernet_addr));
        
        info!("  Interface config created successfully");
        TestResult::Pass
    }

    /// Test that smoltcp can process incoming packets via the Device trait
    pub fn test_interface_poll() -> TestResult {
        info!("  Polling interface for 2 seconds...");
        
        let mac = critical_section::with(|cs| {
            if let Some(ref emac) = *EMAC.borrow_ref_mut(cs) {
                Some(*emac.mac_address())
            } else {
                None
            }
        });

        let Some(mac) = mac else {
            error!("  EMAC not available");
            return TestResult::Fail;
        };

        let ethernet_addr = EthernetAddress(mac);
        let config = IfaceConfig::new(HardwareAddress::Ethernet(ethernet_addr));

        let result = critical_section::with(|cs| {
            let mut emac_ref = EMAC.borrow_ref_mut(cs);
            if let Some(ref mut emac) = *emac_ref {
                // Create interface
                let mut iface = Interface::new(config, emac, Instant::from_millis(0));
                
                // Configure IP address
                iface.update_ip_addrs(|addrs| {
                    let _ = addrs.push(IpCidr::new(IpAddress::Ipv4(OUR_IP), 24));
                });

                // Create socket storage on stack
                let mut socket_storage: [_; 1] = Default::default();
                let mut sockets = SocketSet::new(&mut socket_storage[..]);
                
                let delay = esp_hal::delay::Delay::new();
                let mut poll_count = 0u32;
                
                for i in 0..200u32 {
                    let timestamp = Instant::from_millis((i * 10) as i64);
                    let poll_result = iface.poll(timestamp, emac, &mut sockets);
                    
                    // Check if any socket state changed
                    if poll_result == PollResult::SocketStateChanged {
                        poll_count += 1;
                    }
                    delay.delay_millis(10);
                }

                info!("  Completed {} poll cycles", poll_count);
                
                // Interface working - poll completed
                TestResult::Pass
            } else {
                error!("  EMAC not available");
                TestResult::Fail
            }
        });

        result
    }

    /// Test smoltcp Device trait implementation
    pub fn test_device_capabilities() -> TestResult {
        info!("  Checking Device trait capabilities...");
        
        use smoltcp::phy::Device;
        
        let result = critical_section::with(|cs| {
            let mut emac_ref = EMAC.borrow_ref_mut(cs);
            if let Some(ref mut emac) = *emac_ref {
                let caps = emac.capabilities();
                
                info!("  MTU: {} bytes", caps.max_transmission_unit);
                info!("  Medium: Ethernet");
                
                let checksums = caps.checksum;
                info!("  IPv4 TX checksum: {:?}", checksums.ipv4);
                info!("  TCP TX checksum: {:?}", checksums.tcp);
                info!("  UDP TX checksum: {:?}", checksums.udp);
                
                // Verify reasonable MTU
                if caps.max_transmission_unit >= 1500 {
                    TestResult::Pass
                } else {
                    error!("  MTU too small: {}", caps.max_transmission_unit);
                    TestResult::Fail
                }
            } else {
                error!("  EMAC not available");
                TestResult::Fail
            }
        });

        result
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    
    info!("");
    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║       WT32-ETH01 Integration Test Suite                      ║");
    info!("║       ph-esp32-mac Driver Verification                       ║");
    info!("╚══════════════════════════════════════════════════════════════╝");
    info!("");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Enable external oscillator
    info!("Enabling external 50MHz oscillator...");
    let clk_pin = Output::new(peripherals.GPIO16, Level::High, OutputConfig::default());
    esp_hal::delay::Delay::new().delay_millis(10);
    info!("Oscillator enabled");
    info!("");

    let mut stats = TestStats::new();
    let mut ctx = TestContext::new(clk_pin);

    // =========================================================================
    // Test Group 1: Register Access
    // =========================================================================
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 1: Register Access");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    run_test!(stats, "EMAC clock enable", register_tests::test_emac_clock_enable());
    run_test!(stats, "DMA registers", register_tests::test_dma_registers_accessible());
    run_test!(stats, "MAC registers", register_tests::test_mac_registers_accessible());
    run_test!(stats, "Extension registers", register_tests::test_extension_registers());

    // =========================================================================
    // Test Group 2: EMAC Initialization  
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 2: EMAC Initialization");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    let emac_ok = run_test!(stats, "EMAC init", init_tests::test_emac_init(&mut ctx)) == TestResult::Pass;
    
    if emac_ok {
        run_test!(stats, "RMII pin config", init_tests::test_rmii_pins());
        run_test!(stats, "DMA descriptor chain", init_tests::test_dma_descriptor_chain());
    } else {
        warn!("  Skipping dependent tests");
        stats.record(TestResult::Skip);
        stats.record(TestResult::Skip);
    }

    // =========================================================================
    // Test Group 3: PHY Communication
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 3: PHY Communication");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    run_test!(stats, "PHY MDIO read", phy_tests::test_phy_mdio_read(&mut ctx));
    run_test!(stats, "PHY init", phy_tests::test_phy_init(&mut ctx));
    let link_ok = run_test!(stats, "PHY link up", phy_tests::test_phy_link_up(&mut ctx, 5000)) == TestResult::Pass;

    // =========================================================================
    // Test Group 4: EMAC Operations
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 4: EMAC Operations");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    if emac_ok && link_ok {
        run_test!(stats, "EMAC start", emac_tests::test_emac_start());
        run_test!(stats, "Packet TX", emac_tests::test_packet_tx());
        run_test!(stats, "Packet RX (3s)", emac_tests::test_packet_rx(3));
        run_test!(stats, "EMAC stop/start", emac_tests::test_emac_stop_start());
    } else {
        warn!("  Skipping - requires EMAC init and link");
        for _ in 0..4 { stats.record(TestResult::Skip); }
    }

    // =========================================================================
    // Test Group 5: Link Status
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 5: Link Status");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    run_test!(stats, "Link status query", link_tests::test_link_status_query(&mut ctx));

    // =========================================================================
    // Test Group 6: smoltcp Integration
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 6: smoltcp Integration");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    if emac_ok && link_ok {
        run_test!(stats, "Interface creation", smoltcp_tests::test_interface_creation());
        run_test!(stats, "Device capabilities", smoltcp_tests::test_device_capabilities());
        run_test!(stats, "Interface poll", smoltcp_tests::test_interface_poll());
    } else {
        warn!("  Skipping - requires EMAC init and link");
        for _ in 0..3 { stats.record(TestResult::Skip); }
    }

    // =========================================================================
    // Test Summary
    // =========================================================================
    info!("");
    info!("══════════════════════════════════════════════════════════════════");
    info!("  TEST SUMMARY");
    info!("══════════════════════════════════════════════════════════════════");
    info!("");
    info!("  Total:   {}", stats.total());
    info!("  Passed:  {} ✓", stats.passed);
    info!("  Failed:  {} ✗", stats.failed);
    info!("  Skipped: {} ○", stats.skipped);
    info!("");
    
    if stats.all_passed() {
        info!("╔══════════════════════════════════════════════════════════════╗");
        info!("║                    ALL TESTS PASSED! ✓                       ║");
        info!("╚══════════════════════════════════════════════════════════════╝");
    } else {
        error!("╔══════════════════════════════════════════════════════════════╗");
        error!("║                  SOME TESTS FAILED! ✗                        ║");
        error!("╚══════════════════════════════════════════════════════════════╝");
    }
    info!("");

    // =========================================================================
    // Continuous Monitoring Mode
    // =========================================================================
    info!("Entering continuous RX monitoring mode...");
    info!("(Press reset to restart tests)");
    info!("");

    let mut rx_buffer = [0u8; 1600];
    let mut packet_count = 0u32;
    let mut last_report = 0u32;

    loop {
        critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                while emac.rx_available() {
                    if let Ok(len) = emac.receive(&mut rx_buffer) {
                        packet_count += 1;
                        if len >= 14 {
                            let src = &rx_buffer[6..12];
                            let dst = &rx_buffer[0..6];
                            let etype = u16::from_be_bytes([rx_buffer[12], rx_buffer[13]]);
                            
                            let type_str = match etype {
                                0x0800 => "IPv4",
                                0x0806 => "ARP",
                                0x86DD => "IPv6",
                                0x88CC => "LLDP",
                                _ => "",
                            };
                            
                            info!("RX #{}: {}B {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}->{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} [0x{:04X}] {}",
                                packet_count, len,
                                src[0], src[1], src[2], src[3], src[4], src[5],
                                dst[0], dst[1], dst[2], dst[3], dst[4], dst[5],
                                etype, type_str);
                        }
                    }
                }
            }
        });

        last_report += 1;
        if last_report >= 10000 {
            last_report = 0;
            info!("--- {} packets total ---", packet_count);
            
            if let Ok(up) = ctx.phy.is_link_up(&mut ctx.mdio) {
                info!("Link: {}", if up { "UP" } else { "DOWN" });
            }
        }

        esp_hal::delay::Delay::new().delay_micros(100);
    }
}
