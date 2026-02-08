//! Test Framework
//!
//! Common types, macros, and utilities for integration tests.

use core::cell::RefCell;
use critical_section::Mutex;

use esp_hal::gpio::Output;
use ph_esp32_mac::{Duplex, Emac, Lan8720a, Speed};
use ph_esp32_mac::boards::wt32_eth01::Wt32Eth01;
use ph_esp32_mac::hal::MdioController;

// =============================================================================
// Test Result
// =============================================================================

/// Test result type
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TestResult {
    Pass,
    Fail,
    Skip,
}

impl TestResult {
    pub fn symbol(&self) -> &'static str {
        match self {
            TestResult::Pass => "✓",
            TestResult::Fail => "✗",
            TestResult::Skip => "○",
        }
    }
}

// =============================================================================
// Test Statistics
// =============================================================================

/// Accumulated test statistics
pub struct TestStats {
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
}

impl TestStats {
    pub const fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
            skipped: 0,
        }
    }

    pub fn record(&mut self, result: TestResult) {
        match result {
            TestResult::Pass => self.passed += 1,
            TestResult::Fail => self.failed += 1,
            TestResult::Skip => self.skipped += 1,
        }
    }

    pub fn total(&self) -> u32 {
        self.passed + self.failed + self.skipped
    }

    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }
}

// =============================================================================
// Test Context
// =============================================================================

/// Shared state across tests
pub struct TestContext<'a> {
    pub phy: Lan8720a,
    pub mdio: MdioController<esp_hal::delay::Delay>,
    #[allow(dead_code)]
    pub clk_pin: Option<Output<'a>>,
    pub link_speed: Speed,
    pub link_duplex: Duplex,
    pub emac_initialized: bool,
    pub link_up: bool,
}

impl<'a> TestContext<'a> {
    pub fn new(clk_pin: Output<'a>) -> Self {
        Self {
            phy: Wt32Eth01::lan8720a(),
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
// Static EMAC Instance
// =============================================================================

/// Static EMAC instance with 4 RX/TX buffers and 1600 byte frames
/// 
/// We use 4 buffers for minimal memory footprint during tests.
/// Production code typically uses 8-16 buffers.
pub static EMAC: Mutex<RefCell<Option<Emac<4, 4, 1600>>>> = Mutex::new(RefCell::new(None));

// =============================================================================
// Hardware Constants
// =============================================================================

pub const IO_MUX_BASE: u32 = 0x3FF4_9000;
pub const DMA_BASE: u32 = 0x3FF6_9000;
pub const MAC_BASE: u32 = 0x3FF6_A000;
pub const DPORT_WIFI_CLK_EN: u32 = 0x3FF0_00CC;
