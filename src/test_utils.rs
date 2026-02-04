//! Testing utilities and mock implementations
//!
//! This module provides mock implementations for testing the EMAC driver
//! on the host without hardware access.
//!
//! Only available when running `cargo test`.

// Note: The #[cfg(test)] attribute is applied in lib.rs where this module is declared
#![allow(missing_docs)]
#![allow(clippy::std_instead_of_core, clippy::std_instead_of_alloc)]

extern crate std;

use core::cell::RefCell;
use std::collections::HashMap;
use std::vec;
use std::vec::Vec;

use crate::error::Result;
use crate::hal::mdio::MdioBus;

// Use the internal PHY register constants
use crate::internal::phy_registers::{anlpar, bmsr, phy_reg};

// =============================================================================
// Mock MDIO Bus
// =============================================================================

/// Mock MDIO bus for testing PHY drivers without hardware
///
/// This allows setting up expected register values and verifying writes.
///
/// # Example
///
/// ```ignore
/// let mut mdio = MockMdioBus::new();
/// mdio.set_register(0, 0x01, 0x786D); // Set BMSR with link up
///
/// let phy = Lan8720a::new(0);
/// assert!(phy.is_link_up(&mut mdio).unwrap());
/// ```
#[derive(Debug, Default)]
pub struct MockMdioBus {
    /// Register values: (phy_addr, reg_addr) -> value
    registers: RefCell<HashMap<(u8, u8), u16>>,
    /// Record of writes: (phy_addr, reg_addr, value)
    write_log: RefCell<Vec<(u8, u8, u16)>>,
    /// Whether the bus should report as busy
    busy: RefCell<bool>,
}

impl MockMdioBus {
    /// Create a new mock MDIO bus
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a register value
    pub fn set_register(&self, phy_addr: u8, reg_addr: u8, value: u16) {
        self.registers
            .borrow_mut()
            .insert((phy_addr, reg_addr), value);
    }

    /// Get the current value of a register (for test verification)
    pub fn get_register(&self, phy_addr: u8, reg_addr: u8) -> Option<u16> {
        self.registers.borrow().get(&(phy_addr, reg_addr)).copied()
    }

    /// Get all writes that have been made
    pub fn get_writes(&self) -> Vec<(u8, u8, u16)> {
        self.write_log.borrow().clone()
    }

    /// Clear the write log
    pub fn clear_writes(&self) {
        self.write_log.borrow_mut().clear();
    }

    /// Set the busy flag
    pub fn set_busy(&self, busy: bool) {
        *self.busy.borrow_mut() = busy;
    }

    /// Setup for a LAN8720A PHY with default register values
    pub fn setup_lan8720a(&self, phy_addr: u8) {
        // PHY ID registers (LAN8720A)
        self.set_register(phy_addr, phy_reg::PHYIDR1, 0x0007);
        self.set_register(phy_addr, phy_reg::PHYIDR2, 0xC0F1);

        // BMSR: basic capabilities, link down initially
        let bmsr_value = bmsr::TX_FD_CAPABLE
            | bmsr::TX_HD_CAPABLE
            | bmsr::T10_FD_CAPABLE
            | bmsr::T10_HD_CAPABLE
            | bmsr::AN_ABILITY
            | bmsr::EXT_CAPABLE;
        self.set_register(phy_addr, phy_reg::BMSR, bmsr_value);

        // BMCR: auto-neg enabled
        self.set_register(phy_addr, phy_reg::BMCR, 0x1000);

        // ANAR: advertise all capabilities
        self.set_register(phy_addr, phy_reg::ANAR, 0x01E1);

        // ANLPAR: partner not advertising yet
        self.set_register(phy_addr, phy_reg::ANLPAR, 0x0000);
    }

    /// Simulate link coming up with 100 Mbps Full Duplex
    pub fn simulate_link_up_100_fd(&self, phy_addr: u8) {
        // Update BMSR with link status
        let mut bmsr_val = self.get_register(phy_addr, phy_reg::BMSR).unwrap_or(0);
        bmsr_val |= bmsr::LINK_STATUS | bmsr::AN_COMPLETE;
        self.set_register(phy_addr, phy_reg::BMSR, bmsr_val);

        // Update ANLPAR with partner capabilities
        let anlpar_val = anlpar::SELECTOR_802_3
            | anlpar::CAN_100_FD
            | anlpar::CAN_100_HD
            | anlpar::CAN_10_FD
            | anlpar::CAN_10_HD;
        self.set_register(phy_addr, phy_reg::ANLPAR, anlpar_val);
    }

    /// Simulate link coming up with 10 Mbps Half Duplex
    pub fn simulate_link_up_10_hd(&self, phy_addr: u8) {
        let mut bmsr_val = self.get_register(phy_addr, phy_reg::BMSR).unwrap_or(0);
        bmsr_val |= bmsr::LINK_STATUS | bmsr::AN_COMPLETE;
        self.set_register(phy_addr, phy_reg::BMSR, bmsr_val);

        // Partner only supports 10 Mbps HD
        let anlpar_val = anlpar::SELECTOR_802_3 | anlpar::CAN_10_HD;
        self.set_register(phy_addr, phy_reg::ANLPAR, anlpar_val);
    }

    /// Simulate link going down
    pub fn simulate_link_down(&self, phy_addr: u8) {
        let mut bmsr_val = self.get_register(phy_addr, phy_reg::BMSR).unwrap_or(0);
        bmsr_val &= !(bmsr::LINK_STATUS | bmsr::AN_COMPLETE);
        self.set_register(phy_addr, phy_reg::BMSR, bmsr_val);
        self.set_register(phy_addr, phy_reg::ANLPAR, 0x0000);
    }
}

impl MdioBus for MockMdioBus {
    fn read(&mut self, phy_addr: u8, reg_addr: u8) -> Result<u16> {
        // Return from register map (default 0 if not set)
        Ok(self
            .registers
            .borrow()
            .get(&(phy_addr, reg_addr))
            .copied()
            .unwrap_or(0))
    }

    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16) -> Result<()> {
        // Log the write
        self.write_log
            .borrow_mut()
            .push((phy_addr, reg_addr, value));

        // Actually update the register
        self.registers
            .borrow_mut()
            .insert((phy_addr, reg_addr), value);

        Ok(())
    }

    fn is_busy(&self) -> bool {
        *self.busy.borrow()
    }
}

// =============================================================================
// Mock Delay
// =============================================================================

/// Mock delay for testing without actual timing
///
/// Records delays for verification without actually waiting.
#[derive(Debug, Default)]
pub struct MockDelay {
    /// Total nanoseconds delayed
    total_ns: RefCell<u64>,
}

impl MockDelay {
    /// Create a new mock delay
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total nanoseconds that were "delayed"
    pub fn total_ns(&self) -> u64 {
        *self.total_ns.borrow()
    }

    /// Get total milliseconds that were "delayed"
    pub fn total_ms(&self) -> u64 {
        self.total_ns() / 1_000_000
    }

    /// Reset the delay counter
    pub fn reset(&self) {
        *self.total_ns.borrow_mut() = 0;
    }
}

impl embedded_hal::delay::DelayNs for MockDelay {
    fn delay_ns(&mut self, ns: u32) {
        *self.total_ns.borrow_mut() += ns as u64;
    }
}

// =============================================================================
// Mock DMA Descriptor
// =============================================================================

/// Mock DMA descriptor for testing DMA flow logic without hardware
///
/// This allows simulating DMA descriptor behavior for TX/RX flow testing
/// without requiring actual ESP32 hardware access.
///
/// # Example
///
/// ```ignore
/// use crate::test_utils::MockDescriptor;
/// use crate::dma::DescriptorRing;
///
/// let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing::from_array(
///     [MockDescriptor::new(); 4]
/// );
///
/// // Simulate DMA receiving a frame
/// ring.get_mut(0).simulate_receive(1500);
/// assert!(!ring.get(0).is_owned());
/// assert_eq!(ring.get(0).frame_length(), 1500);
/// ```
#[derive(Clone, Copy, Default, Debug)]
pub struct MockDescriptor {
    /// Whether the descriptor is owned by DMA (true) or CPU (false)
    pub owned: bool,
    /// Whether this is the first descriptor of a frame
    pub first: bool,
    /// Whether this is the last descriptor of a frame
    pub last: bool,
    /// Whether the descriptor has an error
    pub has_error: bool,
    /// Frame length (valid when `last` is true)
    pub frame_len: usize,
}

impl MockDescriptor {
    /// Create a new mock descriptor with default (empty) state
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if descriptor is owned by DMA
    pub fn is_owned(&self) -> bool {
        self.owned
    }

    /// Give descriptor ownership to DMA
    pub fn set_owned(&mut self) {
        self.owned = true;
    }

    /// Take ownership from DMA (for CPU use)
    pub fn clear_owned(&mut self) {
        self.owned = false;
    }

    /// Check if this is the first descriptor of a frame
    pub fn is_first(&self) -> bool {
        self.first
    }

    /// Check if this is the last descriptor of a frame
    pub fn is_last(&self) -> bool {
        self.last
    }

    /// Check if frame has any errors
    pub fn has_error(&self) -> bool {
        self.has_error
    }

    /// Get received frame length
    pub fn frame_length(&self) -> usize {
        self.frame_len
    }

    /// Simulate receiving a complete frame
    ///
    /// This sets the descriptor state as if DMA had received a complete frame:
    /// - Ownership released to CPU
    /// - First and last flags set (single-descriptor frame)
    /// - No errors
    /// - Frame length set to `len`
    pub fn simulate_receive(&mut self, len: usize) {
        self.owned = false; // DMA releases ownership
        self.first = true;
        self.last = true;
        self.has_error = false;
        self.frame_len = len;
    }

    /// Simulate a receive error
    ///
    /// This sets the descriptor state as if DMA had encountered an error:
    /// - Ownership released to CPU
    /// - First and last flags set
    /// - Error flag set
    /// - Frame length set to 0
    pub fn simulate_error(&mut self) {
        self.owned = false;
        self.first = true;
        self.last = true;
        self.has_error = true;
        self.frame_len = 0;
    }

    /// Simulate a multi-descriptor frame fragment
    ///
    /// This sets the descriptor state for part of a multi-descriptor frame:
    /// - Ownership released to CPU
    /// - First/last flags set according to parameters
    /// - No errors
    /// - Frame length set (only valid on last descriptor)
    pub fn simulate_fragment(&mut self, first: bool, last: bool, len: usize) {
        self.owned = false;
        self.first = first;
        self.last = last;
        self.has_error = false;
        self.frame_len = len;
    }

    /// Reset the descriptor to initial state
    ///
    /// Returns descriptor to the state as if it was newly created:
    /// - Not owned by DMA
    /// - No first/last flags
    /// - No errors
    /// - Zero frame length
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Recycle the descriptor for reuse
    ///
    /// Prepares descriptor to be given back to DMA for receiving:
    /// - Not owned yet (call `set_owned()` to give to DMA)
    /// - Clears all status flags
    pub fn recycle(&mut self) {
        self.first = false;
        self.last = false;
        self.has_error = false;
        self.frame_len = 0;
    }
}

// =============================================================================
// Test Assertions
// =============================================================================

/// Assert that a register was written with a specific value
#[macro_export]
macro_rules! assert_reg_written {
    ($mdio:expr, $phy:expr, $reg:expr, $value:expr) => {
        let writes = $mdio.get_writes();
        assert!(
            writes
                .iter()
                .any(|w| w.0 == $phy && w.1 == $reg && w.2 == $value),
            "Expected write to PHY {} reg {} with value 0x{:04X}, but got: {:?}",
            $phy,
            $reg,
            $value,
            writes
        );
    };
}

/// Assert that a register was written (any value)
#[macro_export]
macro_rules! assert_reg_written_any {
    ($mdio:expr, $phy:expr, $reg:expr) => {
        let writes = $mdio.get_writes();
        assert!(
            writes.iter().any(|w| w.0 == $phy && w.1 == $reg),
            "Expected write to PHY {} reg {}, but got: {:?}",
            $phy,
            $reg,
            writes
        );
    };
}

// =============================================================================
// PHY Register Constants for Testing
// =============================================================================

/// Common PHY register addresses for testing
pub mod phy_regs {
    pub const BMCR: u8 = 0x00;
    pub const BMSR: u8 = 0x01;
    pub const PHYIDR1: u8 = 0x02;
    pub const PHYIDR2: u8 = 0x03;
    pub const ANAR: u8 = 0x04;
    pub const ANLPAR: u8 = 0x05;
}

/// BMCR bit definitions for testing
pub mod bmcr_bits {
    pub const RESET: u16 = 1 << 15;
    pub const LOOPBACK: u16 = 1 << 14;
    pub const SPEED_100: u16 = 1 << 13;
    pub const AUTO_NEG_ENABLE: u16 = 1 << 12;
    pub const POWER_DOWN: u16 = 1 << 11;
    pub const ISOLATE: u16 = 1 << 10;
    pub const RESTART_AUTO_NEG: u16 = 1 << 9;
    pub const DUPLEX_FULL: u16 = 1 << 8;
}

/// BMSR bit definitions for testing
pub mod bmsr_bits {
    pub const CAN_100_T4: u16 = 1 << 15;
    pub const CAN_100_FD: u16 = 1 << 14;
    pub const CAN_100_HD: u16 = 1 << 13;
    pub const CAN_10_FD: u16 = 1 << 12;
    pub const CAN_10_HD: u16 = 1 << 11;
    pub const AUTO_NEG_COMPLETE: u16 = 1 << 5;
    pub const REMOTE_FAULT: u16 = 1 << 4;
    pub const CAN_AUTO_NEG: u16 = 1 << 3;
    pub const LINK_STATUS: u16 = 1 << 2;
    pub const JABBER_DETECT: u16 = 1 << 1;
    pub const EXTENDED_CAPABLE: u16 = 1 << 0;
}

/// ANLPAR bit definitions for testing
pub mod anlpar_bits {
    pub const NEXT_PAGE: u16 = 1 << 15;
    pub const ACK: u16 = 1 << 14;
    pub const REMOTE_FAULT: u16 = 1 << 13;
    pub const PAUSE_ASYM: u16 = 1 << 11;
    pub const PAUSE: u16 = 1 << 10;
    pub const CAN_100_T4: u16 = 1 << 9;
    pub const CAN_100_FD: u16 = 1 << 8;
    pub const CAN_100_HD: u16 = 1 << 7;
    pub const CAN_10_FD: u16 = 1 << 6;
    pub const CAN_10_HD: u16 = 1 << 5;
    pub const SELECTOR_MASK: u16 = 0x1F;
    pub const SELECTOR_802_3: u16 = 0x01;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_mdio_read_write() {
        let mut mdio = MockMdioBus::new();

        // Initially reads 0
        assert_eq!(mdio.read(0, 1).unwrap(), 0);

        // Set a value
        mdio.set_register(0, 1, 0x1234);
        assert_eq!(mdio.read(0, 1).unwrap(), 0x1234);

        // Write updates the value
        mdio.write(0, 1, 0x5678).unwrap();
        assert_eq!(mdio.read(0, 1).unwrap(), 0x5678);

        // Write is logged
        assert_eq!(mdio.get_writes(), vec![(0, 1, 0x5678)]);
    }

    #[test]
    fn mock_mdio_multiple_phys() {
        let mut mdio = MockMdioBus::new();

        mdio.set_register(0, 1, 0x1111);
        mdio.set_register(1, 1, 0x2222);

        assert_eq!(mdio.read(0, 1).unwrap(), 0x1111);
        assert_eq!(mdio.read(1, 1).unwrap(), 0x2222);
    }

    #[test]
    fn mock_delay_tracking() {
        let mut delay = MockDelay::new();

        embedded_hal::delay::DelayNs::delay_ns(&mut delay, 1000);
        embedded_hal::delay::DelayNs::delay_ns(&mut delay, 2000);

        assert_eq!(delay.total_ns(), 3000);
        assert_eq!(delay.total_ms(), 0); // Less than 1ms

        embedded_hal::delay::DelayNs::delay_ns(&mut delay, 1_000_000);
        assert_eq!(delay.total_ms(), 1);
    }

    #[test]
    fn mock_mdio_lan8720a_setup() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        // Check PHY ID
        assert_eq!(mdio.read(0, phy_regs::PHYIDR1).unwrap(), 0x0007);
        assert_eq!(mdio.read(0, phy_regs::PHYIDR2).unwrap(), 0xC0F1);

        // Check BMSR has capabilities but no link
        let bmsr = mdio.read(0, phy_regs::BMSR).unwrap();
        assert!(bmsr & bmsr_bits::CAN_100_FD != 0);
        assert!(bmsr & bmsr_bits::LINK_STATUS == 0);
    }

    #[test]
    fn mock_mdio_link_simulation() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        // Initially link is down
        let bmsr = mdio.read(0, phy_regs::BMSR).unwrap();
        assert!(bmsr & bmsr_bits::LINK_STATUS == 0);

        // Simulate link up
        mdio.simulate_link_up_100_fd(0);

        let bmsr = mdio.read(0, phy_regs::BMSR).unwrap();
        assert!(bmsr & bmsr_bits::LINK_STATUS != 0);
        assert!(bmsr & bmsr_bits::AUTO_NEG_COMPLETE != 0);

        let anlpar = mdio.read(0, phy_regs::ANLPAR).unwrap();
        assert!(anlpar & anlpar_bits::CAN_100_FD != 0);

        // Simulate link down
        mdio.simulate_link_down(0);

        let bmsr = mdio.read(0, phy_regs::BMSR).unwrap();
        assert!(bmsr & bmsr_bits::LINK_STATUS == 0);
    }
}
