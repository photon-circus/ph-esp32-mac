//! MDIO (Management Data Input/Output) HAL
//!
//! This module provides a higher-level abstraction for the MDIO interface
//! used to communicate with Ethernet PHYs. It wraps the low-level SMI
//! (Station Management Interface) register access.

use embedded_hal::delay::DelayNs;

use crate::error::{ConfigError, IoError, Result};
use crate::register::mac::{
    MacRegs, GMACMIIADDR_CR_MASK, GMACMIIADDR_CR_SHIFT, GMACMIIADDR_GB,
    GMACMIIADDR_GR_MASK, GMACMIIADDR_GR_SHIFT, GMACMIIADDR_GW, GMACMIIADDR_PA_MASK,
    GMACMIIADDR_PA_SHIFT,
};

// =============================================================================
// MDIO Constants
// =============================================================================

/// Default MDIO operation timeout in microseconds
pub const MDIO_TIMEOUT_US: u32 = 1_000;

/// Maximum valid PHY address (5-bit field)
pub const MAX_PHY_ADDR: u8 = 31;

/// Maximum valid register address (5-bit field)
pub const MAX_REG_ADDR: u8 = 31;

/// MDC clock divider values based on system clock
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum MdcClockDivider {
    /// Clock/42 (60-100 MHz system clock)
    Div42 = 0,
    /// Clock/62 (100-150 MHz system clock)
    Div62 = 1,
    /// Clock/16 (20-35 MHz system clock)
    Div16 = 2,
    /// Clock/26 (35-60 MHz system clock)
    Div26 = 3,
    /// Clock/102 (150-250 MHz system clock)
    #[default]
    Div102 = 4,
    /// Clock/124 (250-300 MHz system clock)
    Div124 = 5,
}

impl MdcClockDivider {
    /// Get the appropriate divider for a given system clock frequency
    ///
    /// The MDC clock must not exceed 2.5 MHz per IEEE 802.3.
    pub const fn from_sys_clock_hz(sys_clk_hz: u32) -> Self {
        if sys_clk_hz < 35_000_000 {
            Self::Div16
        } else if sys_clk_hz < 60_000_000 {
            Self::Div26
        } else if sys_clk_hz < 100_000_000 {
            Self::Div42
        } else if sys_clk_hz < 150_000_000 {
            Self::Div62
        } else if sys_clk_hz < 250_000_000 {
            Self::Div102
        } else {
            Self::Div124
        }
    }

    /// Get the divider value for register programming
    pub const fn to_reg_value(self) -> u32 {
        self as u32
    }
}

// =============================================================================
// MDIO Bus Trait
// =============================================================================

/// Trait for MDIO bus operations
///
/// This trait can be implemented by different backends, allowing
/// the PHY driver to work with various MDIO implementations.
pub trait MdioBus {
    /// Read a PHY register
    fn read(&mut self, phy_addr: u8, reg_addr: u8) -> Result<u16>;

    /// Write a PHY register
    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16) -> Result<()>;

    /// Check if the MDIO bus is busy
    fn is_busy(&self) -> bool;
}

// =============================================================================
// MDIO Controller
// =============================================================================

/// MDIO controller for PHY register access
///
/// This controller handles the low-level MDIO communication protocol
/// using the ESP32 EMAC's built-in SMI interface.
#[derive(Debug)]
pub struct MdioController<D: DelayNs> {
    /// Clock divider setting
    clock_divider: MdcClockDivider,
    /// Delay provider for timeout handling
    delay: D,
    /// Operation timeout in microseconds
    timeout_us: u32,
}

impl<D: DelayNs> MdioController<D> {
    /// Create a new MDIO controller with the specified delay
    pub fn new(delay: D) -> Self {
        Self {
            clock_divider: MdcClockDivider::Div102,
            timeout_us: MDIO_TIMEOUT_US,
            delay,
        }
    }

    /// Create a new MDIO controller with custom clock divider
    pub fn with_clock_divider(delay: D, divider: MdcClockDivider) -> Self {
        Self {
            clock_divider: divider,
            timeout_us: MDIO_TIMEOUT_US,
            delay,
        }
    }

    /// Set the clock divider based on system clock frequency
    pub fn configure_for_sys_clock(&mut self, sys_clk_hz: u32) {
        self.clock_divider = MdcClockDivider::from_sys_clock_hz(sys_clk_hz);
    }

    /// Set the operation timeout
    pub fn set_timeout_us(&mut self, timeout_us: u32) {
        self.timeout_us = timeout_us;
    }

    /// Wait for MDIO operation to complete
    fn wait_not_busy(&mut self) -> Result<()> {
        let mut elapsed = 0u32;
        while MacRegs::mii_address() & GMACMIIADDR_GB != 0 {
            if elapsed >= self.timeout_us {
                return Err(IoError::Timeout.into());
            }
            self.delay.delay_us(10);
            elapsed += 10;
        }
        Ok(())
    }

    /// Build the GMACMIIADDR register value
    fn build_mii_addr(&self, phy_addr: u8, reg_addr: u8, is_write: bool) -> u32 {
        let mut addr = 0u32;

        // PHY address (bits 15:11)
        addr |= ((phy_addr as u32) << GMACMIIADDR_PA_SHIFT) & GMACMIIADDR_PA_MASK;

        // Register address (bits 10:6)
        addr |= ((reg_addr as u32) << GMACMIIADDR_GR_SHIFT) & GMACMIIADDR_GR_MASK;

        // Clock divider (bits 5:2)
        addr |= ((self.clock_divider.to_reg_value()) << GMACMIIADDR_CR_SHIFT) & GMACMIIADDR_CR_MASK;

        // Write flag (bit 1)
        if is_write {
            addr |= GMACMIIADDR_GW;
        }

        // Busy flag (bit 0) - triggers the operation
        addr |= GMACMIIADDR_GB;

        addr
    }
}

impl<D: DelayNs> MdioBus for MdioController<D> {
    fn read(&mut self, phy_addr: u8, reg_addr: u8) -> Result<u16> {
        // Validate addresses
        if phy_addr > MAX_PHY_ADDR {
            return Err(ConfigError::InvalidPhyAddress.into());
        }
        if reg_addr > MAX_REG_ADDR {
            return Err(ConfigError::InvalidConfig.into());
        }

        // Wait for any pending operation
        self.wait_not_busy()?;

        // Build and write the address register (this triggers the read)
        let addr = self.build_mii_addr(phy_addr, reg_addr, false);
        MacRegs::set_mii_address(addr);

        // Wait for the read to complete
        self.wait_not_busy()?;

        // Read the data
        let data = MacRegs::mii_data() & 0xFFFF;
        Ok(data as u16)
    }

    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16) -> Result<()> {
        // Validate addresses
        if phy_addr > MAX_PHY_ADDR {
            return Err(ConfigError::InvalidPhyAddress.into());
        }
        if reg_addr > MAX_REG_ADDR {
            return Err(ConfigError::InvalidConfig.into());
        }

        // Wait for any pending operation
        self.wait_not_busy()?;

        // Write the data first
        MacRegs::set_mii_data(value as u32);

        // Build and write the address register (this triggers the write)
        let addr = self.build_mii_addr(phy_addr, reg_addr, true);
        MacRegs::set_mii_address(addr);

        // Wait for the write to complete
        self.wait_not_busy()
    }

    fn is_busy(&self) -> bool {
        (MacRegs::mii_address() & GMACMIIADDR_GB) != 0
    }
}

// =============================================================================
// PHY Register Definitions (IEEE 802.3 standard registers)
// =============================================================================

/// Standard PHY register addresses (IEEE 802.3 Clause 22)
pub mod phy_reg {
    /// Basic Mode Control Register
    pub const BMCR: u8 = 0;
    /// Basic Mode Status Register
    pub const BMSR: u8 = 1;
    /// PHY Identifier 1
    pub const PHYIDR1: u8 = 2;
    /// PHY Identifier 2
    pub const PHYIDR2: u8 = 3;
    /// Auto-Negotiation Advertisement Register
    pub const ANAR: u8 = 4;
    /// Auto-Negotiation Link Partner Ability Register
    pub const ANLPAR: u8 = 5;
    /// Auto-Negotiation Expansion Register
    pub const ANER: u8 = 6;
    /// Auto-Negotiation Next Page Transmit Register
    pub const ANNPTR: u8 = 7;
    /// Auto-Negotiation Next Page Receive Register
    pub const ANNPRR: u8 = 8;
    /// MMD Access Control Register
    pub const MMD_CTRL: u8 = 13;
    /// MMD Access Data Register
    pub const MMD_DATA: u8 = 14;
    /// Extended Status Register
    pub const ESTATUS: u8 = 15;
}

/// BMCR (Basic Mode Control Register) bits
pub mod bmcr {
    /// Soft reset
    pub const RESET: u16 = 1 << 15;
    /// Loopback mode
    pub const LOOPBACK: u16 = 1 << 14;
    /// Speed select (100 Mbps if set)
    pub const SPEED_100: u16 = 1 << 13;
    /// Auto-negotiation enable
    pub const AN_ENABLE: u16 = 1 << 12;
    /// Power down
    pub const POWER_DOWN: u16 = 1 << 11;
    /// Isolate
    pub const ISOLATE: u16 = 1 << 10;
    /// Restart auto-negotiation
    pub const AN_RESTART: u16 = 1 << 9;
    /// Duplex mode (full duplex if set)
    pub const DUPLEX_FULL: u16 = 1 << 8;
}

/// BMSR (Basic Mode Status Register) bits
pub mod bmsr {
    /// 100BASE-T4 capable
    pub const T4_CAPABLE: u16 = 1 << 15;
    /// 100BASE-TX full duplex capable
    pub const TX_FD_CAPABLE: u16 = 1 << 14;
    /// 100BASE-TX half duplex capable
    pub const TX_HD_CAPABLE: u16 = 1 << 13;
    /// 10BASE-T full duplex capable
    pub const T10_FD_CAPABLE: u16 = 1 << 12;
    /// 10BASE-T half duplex capable
    pub const T10_HD_CAPABLE: u16 = 1 << 11;
    /// Extended status register present
    pub const ESTATUS: u16 = 1 << 8;
    /// MF preamble suppression
    pub const MF_PREAMBLE_SUPP: u16 = 1 << 6;
    /// Auto-negotiation complete
    pub const AN_COMPLETE: u16 = 1 << 5;
    /// Remote fault
    pub const REMOTE_FAULT: u16 = 1 << 4;
    /// Auto-negotiation ability
    pub const AN_ABILITY: u16 = 1 << 3;
    /// Link status
    pub const LINK_STATUS: u16 = 1 << 2;
    /// Jabber detect
    pub const JABBER_DETECT: u16 = 1 << 1;
    /// Extended capabilities
    pub const EXT_CAPABLE: u16 = 1 << 0;
}

/// ANAR (Auto-Negotiation Advertisement Register) bits
pub mod anar {
    /// Next page
    pub const NEXT_PAGE: u16 = 1 << 15;
    /// Acknowledge
    pub const ACK: u16 = 1 << 14;
    /// Remote fault
    pub const REMOTE_FAULT: u16 = 1 << 13;
    /// Pause capable
    pub const PAUSE: u16 = 1 << 10;
    /// 100BASE-T4
    pub const T4: u16 = 1 << 9;
    /// 100BASE-TX full duplex
    pub const TX_FD: u16 = 1 << 8;
    /// 100BASE-TX half duplex
    pub const TX_HD: u16 = 1 << 7;
    /// 10BASE-T full duplex
    pub const T10_FD: u16 = 1 << 6;
    /// 10BASE-T half duplex
    pub const T10_HD: u16 = 1 << 5;
    /// Selector field (IEEE 802.3)
    pub const SELECTOR: u16 = 0x001F;
    /// IEEE 802.3 selector value
    pub const SELECTOR_IEEE802_3: u16 = 0x0001;
}

/// ANLPAR (Auto-Negotiation Link Partner Ability Register) bits
///
/// Same bit layout as ANAR, but represents what the link partner advertises.
pub mod anlpar {
    /// Next page
    pub const NEXT_PAGE: u16 = 1 << 15;
    /// Acknowledge
    pub const ACK: u16 = 1 << 14;
    /// Remote fault
    pub const REMOTE_FAULT: u16 = 1 << 13;
    /// Pause capable
    pub const PAUSE: u16 = 1 << 10;
    /// Asymmetric pause
    pub const PAUSE_ASYM: u16 = 1 << 11;
    /// 100BASE-T4
    pub const CAN_100_T4: u16 = 1 << 9;
    /// 100BASE-TX full duplex
    pub const CAN_100_FD: u16 = 1 << 8;
    /// 100BASE-TX half duplex
    pub const CAN_100_HD: u16 = 1 << 7;
    /// 10BASE-T full duplex
    pub const CAN_10_FD: u16 = 1 << 6;
    /// 10BASE-T half duplex
    pub const CAN_10_HD: u16 = 1 << 5;
    /// Selector field mask
    pub const SELECTOR_MASK: u16 = 0x001F;
    /// IEEE 802.3 selector value
    pub const SELECTOR_802_3: u16 = 0x0001;
}

// =============================================================================
// PHY Helper Functions
// =============================================================================

/// PHY status information
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PhyStatus {
    /// Link is up
    pub link_up: bool,
    /// Auto-negotiation complete
    pub an_complete: bool,
    /// Speed (true = 100 Mbps, false = 10 Mbps)
    pub speed_100: bool,
    /// Duplex (true = full, false = half)
    pub full_duplex: bool,
}

/// Read PHY status from standard registers
pub fn read_phy_status<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<PhyStatus> {
    let bmsr = mdio.read(phy_addr, phy_reg::BMSR)?;
    let bmcr = mdio.read(phy_addr, phy_reg::BMCR)?;

    Ok(PhyStatus {
        link_up: (bmsr & bmsr::LINK_STATUS) != 0,
        an_complete: (bmsr & bmsr::AN_COMPLETE) != 0,
        speed_100: (bmcr & bmcr::SPEED_100) != 0,
        full_duplex: (bmcr & bmcr::DUPLEX_FULL) != 0,
    })
}

/// Perform a soft reset on the PHY
pub fn reset_phy<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<()> {
    mdio.write(phy_addr, phy_reg::BMCR, bmcr::RESET)
}

/// Read the PHY identifier
pub fn read_phy_id<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<u32> {
    let id1 = mdio.read(phy_addr, phy_reg::PHYIDR1)? as u32;
    let id2 = mdio.read(phy_addr, phy_reg::PHYIDR2)? as u32;
    Ok((id1 << 16) | id2)
}

/// Enable auto-negotiation on the PHY
pub fn enable_auto_negotiation<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<()> {
    let bmcr = mdio.read(phy_addr, phy_reg::BMCR)?;
    mdio.write(
        phy_addr,
        phy_reg::BMCR,
        (bmcr | bmcr::AN_ENABLE | bmcr::AN_RESTART) & !bmcr::ISOLATE,
    )
}

/// Force PHY to specific speed and duplex
pub fn force_speed_duplex<M: MdioBus>(
    mdio: &mut M,
    phy_addr: u8,
    speed_100: bool,
    full_duplex: bool,
) -> Result<()> {
    let mut bmcr = mdio.read(phy_addr, phy_reg::BMCR)?;

    // Disable auto-negotiation
    bmcr &= !bmcr::AN_ENABLE;
    bmcr &= !bmcr::ISOLATE;

    // Set speed
    if speed_100 {
        bmcr |= bmcr::SPEED_100;
    } else {
        bmcr &= !bmcr::SPEED_100;
    }

    // Set duplex
    if full_duplex {
        bmcr |= bmcr::DUPLEX_FULL;
    } else {
        bmcr &= !bmcr::DUPLEX_FULL;
    }

    mdio.write(phy_addr, phy_reg::BMCR, bmcr)
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Clock Divider Tests
    // =========================================================================

    #[test]
    fn clock_divider_from_sys_clock() {
        // Low frequency -> Div16
        assert_eq!(MdcClockDivider::from_sys_clock_hz(20_000_000), MdcClockDivider::Div16);

        // Medium frequency -> Div26
        assert_eq!(MdcClockDivider::from_sys_clock_hz(40_000_000), MdcClockDivider::Div26);

        // ESP32 default 80MHz -> Div42
        assert_eq!(MdcClockDivider::from_sys_clock_hz(80_000_000), MdcClockDivider::Div42);

        // 160MHz -> Div102
        assert_eq!(MdcClockDivider::from_sys_clock_hz(160_000_000), MdcClockDivider::Div102);

        // High frequency -> Div124
        assert_eq!(MdcClockDivider::from_sys_clock_hz(280_000_000), MdcClockDivider::Div124);
    }

    #[test]
    fn clock_divider_to_reg_value() {
        assert_eq!(MdcClockDivider::Div42.to_reg_value(), 0);
        assert_eq!(MdcClockDivider::Div62.to_reg_value(), 1);
        assert_eq!(MdcClockDivider::Div16.to_reg_value(), 2);
        assert_eq!(MdcClockDivider::Div26.to_reg_value(), 3);
        assert_eq!(MdcClockDivider::Div102.to_reg_value(), 4);
        assert_eq!(MdcClockDivider::Div124.to_reg_value(), 5);
    }

    #[test]
    fn clock_divider_default() {
        assert_eq!(MdcClockDivider::default(), MdcClockDivider::Div102);
    }

    #[test]
    fn clock_divider_boundary_35mhz() {
        // Just under 35MHz -> Div16
        assert_eq!(MdcClockDivider::from_sys_clock_hz(34_999_999), MdcClockDivider::Div16);
        // At 35MHz -> Div26
        assert_eq!(MdcClockDivider::from_sys_clock_hz(35_000_000), MdcClockDivider::Div26);
    }

    #[test]
    fn clock_divider_boundary_60mhz() {
        // Just under 60MHz -> Div26
        assert_eq!(MdcClockDivider::from_sys_clock_hz(59_999_999), MdcClockDivider::Div26);
        // At 60MHz -> Div42
        assert_eq!(MdcClockDivider::from_sys_clock_hz(60_000_000), MdcClockDivider::Div42);
    }

    #[test]
    fn clock_divider_boundary_100mhz() {
        // Just under 100MHz -> Div42
        assert_eq!(MdcClockDivider::from_sys_clock_hz(99_999_999), MdcClockDivider::Div42);
        // At 100MHz -> Div62
        assert_eq!(MdcClockDivider::from_sys_clock_hz(100_000_000), MdcClockDivider::Div62);
    }

    #[test]
    fn clock_divider_boundary_150mhz() {
        // Just under 150MHz -> Div62
        assert_eq!(MdcClockDivider::from_sys_clock_hz(149_999_999), MdcClockDivider::Div62);
        // At 150MHz -> Div102
        assert_eq!(MdcClockDivider::from_sys_clock_hz(150_000_000), MdcClockDivider::Div102);
    }

    #[test]
    fn clock_divider_boundary_250mhz() {
        // Just under 250MHz -> Div102
        assert_eq!(MdcClockDivider::from_sys_clock_hz(249_999_999), MdcClockDivider::Div102);
        // At 250MHz -> Div124
        assert_eq!(MdcClockDivider::from_sys_clock_hz(250_000_000), MdcClockDivider::Div124);
    }

    // =========================================================================
    // MDIO Constants Tests
    // =========================================================================

    #[test]
    fn mdio_timeout_is_reasonable() {
        assert!(MDIO_TIMEOUT_US > 0);
        assert!(MDIO_TIMEOUT_US <= 10_000); // Max 10ms
    }

    #[test]
    fn max_phy_addr_is_5_bits() {
        assert_eq!(MAX_PHY_ADDR, 31);
        assert!(MAX_PHY_ADDR < 32); // 5-bit field
    }

    #[test]
    fn max_reg_addr_is_5_bits() {
        assert_eq!(MAX_REG_ADDR, 31);
        assert!(MAX_REG_ADDR < 32); // 5-bit field
    }

    // =========================================================================
    // PHY Register Address Tests
    // =========================================================================

    #[test]
    fn phy_reg_standard_addresses() {
        // IEEE 802.3 Clause 22 standard register addresses
        assert_eq!(phy_reg::BMCR, 0);
        assert_eq!(phy_reg::BMSR, 1);
        assert_eq!(phy_reg::PHYIDR1, 2);
        assert_eq!(phy_reg::PHYIDR2, 3);
        assert_eq!(phy_reg::ANAR, 4);
        assert_eq!(phy_reg::ANLPAR, 5);
    }

    #[test]
    fn phy_reg_extended_addresses() {
        assert_eq!(phy_reg::ANER, 6);
        assert_eq!(phy_reg::ANNPTR, 7);
        assert_eq!(phy_reg::ANNPRR, 8);
        assert_eq!(phy_reg::MMD_CTRL, 13);
        assert_eq!(phy_reg::MMD_DATA, 14);
        assert_eq!(phy_reg::ESTATUS, 15);
    }

    #[test]
    fn phy_reg_all_valid() {
        // All addresses should be within valid range
        assert!(phy_reg::BMCR <= MAX_REG_ADDR);
        assert!(phy_reg::BMSR <= MAX_REG_ADDR);
        assert!(phy_reg::PHYIDR1 <= MAX_REG_ADDR);
        assert!(phy_reg::PHYIDR2 <= MAX_REG_ADDR);
        assert!(phy_reg::ANAR <= MAX_REG_ADDR);
        assert!(phy_reg::ANLPAR <= MAX_REG_ADDR);
        assert!(phy_reg::ESTATUS <= MAX_REG_ADDR);
    }

    // =========================================================================
    // BMCR Bit Tests
    // =========================================================================

    #[test]
    fn bmcr_reset_bit() {
        assert_eq!(bmcr::RESET, 0x8000);
    }

    #[test]
    fn bmcr_bit_positions() {
        assert_eq!(bmcr::RESET, 1 << 15);
        assert_eq!(bmcr::LOOPBACK, 1 << 14);
        assert_eq!(bmcr::SPEED_100, 1 << 13);
        assert_eq!(bmcr::AN_ENABLE, 1 << 12);
        assert_eq!(bmcr::POWER_DOWN, 1 << 11);
        assert_eq!(bmcr::ISOLATE, 1 << 10);
        assert_eq!(bmcr::AN_RESTART, 1 << 9);
        assert_eq!(bmcr::DUPLEX_FULL, 1 << 8);
    }

    #[test]
    fn bmcr_speed_duplex_bits() {
        // 100 Mbps Full Duplex
        let bmcr_100fd = bmcr::SPEED_100 | bmcr::DUPLEX_FULL;
        assert!(bmcr_100fd & bmcr::SPEED_100 != 0);
        assert!(bmcr_100fd & bmcr::DUPLEX_FULL != 0);

        // 10 Mbps Half Duplex
        let bmcr_10hd = 0u16;
        assert!(bmcr_10hd & bmcr::SPEED_100 == 0);
        assert!(bmcr_10hd & bmcr::DUPLEX_FULL == 0);
    }

    #[test]
    fn bmcr_auto_neg_bits() {
        let bmcr_an = bmcr::AN_ENABLE | bmcr::AN_RESTART;
        assert!(bmcr_an & bmcr::AN_ENABLE != 0);
        assert!(bmcr_an & bmcr::AN_RESTART != 0);
    }

    #[test]
    fn bmcr_bits_are_distinct() {
        // Verify no bits overlap
        let all_bits = bmcr::RESET
            | bmcr::LOOPBACK
            | bmcr::SPEED_100
            | bmcr::AN_ENABLE
            | bmcr::POWER_DOWN
            | bmcr::ISOLATE
            | bmcr::AN_RESTART
            | bmcr::DUPLEX_FULL;
        // Count the bits
        assert_eq!(all_bits.count_ones(), 8);
    }

    // =========================================================================
    // BMSR Bit Parsing Tests
    // =========================================================================

    #[test]
    fn bmsr_link_status_bit() {
        // Link up
        let bmsr_up = 0x786D;
        assert!(bmsr_up & bmsr::LINK_STATUS != 0);

        // Link down (bit 2 clear)
        let bmsr_down = 0x7869;
        assert!(bmsr_down & bmsr::LINK_STATUS == 0);
    }

    #[test]
    fn bmsr_auto_neg_complete_bit() {
        // AN complete (bit 5 set)
        let bmsr_complete = 0x0024;
        assert!(bmsr_complete & bmsr::AN_COMPLETE != 0);

        // AN not complete
        let bmsr_pending = 0x0004;
        assert!(bmsr_pending & bmsr::AN_COMPLETE == 0);
    }

    #[test]
    fn bmsr_capability_bits() {
        // Full capability PHY
        let bmsr = bmsr::TX_FD_CAPABLE
            | bmsr::TX_HD_CAPABLE
            | bmsr::T10_FD_CAPABLE
            | bmsr::T10_HD_CAPABLE
            | bmsr::AN_ABILITY;

        assert!(bmsr & bmsr::TX_FD_CAPABLE != 0);
        assert!(bmsr & bmsr::TX_HD_CAPABLE != 0);
        assert!(bmsr & bmsr::T10_FD_CAPABLE != 0);
        assert!(bmsr & bmsr::T10_HD_CAPABLE != 0);
        assert!(bmsr & bmsr::AN_ABILITY != 0);
    }

    #[test]
    fn bmsr_bit_positions() {
        assert_eq!(bmsr::T4_CAPABLE, 1 << 15);
        assert_eq!(bmsr::TX_FD_CAPABLE, 1 << 14);
        assert_eq!(bmsr::TX_HD_CAPABLE, 1 << 13);
        assert_eq!(bmsr::T10_FD_CAPABLE, 1 << 12);
        assert_eq!(bmsr::T10_HD_CAPABLE, 1 << 11);
        assert_eq!(bmsr::ESTATUS, 1 << 8);
        assert_eq!(bmsr::AN_COMPLETE, 1 << 5);
        assert_eq!(bmsr::REMOTE_FAULT, 1 << 4);
        assert_eq!(bmsr::AN_ABILITY, 1 << 3);
        assert_eq!(bmsr::LINK_STATUS, 1 << 2);
        assert_eq!(bmsr::JABBER_DETECT, 1 << 1);
        assert_eq!(bmsr::EXT_CAPABLE, 1 << 0);
    }

    // =========================================================================
    // ANAR Bit Tests
    // =========================================================================

    #[test]
    fn anar_bit_positions() {
        assert_eq!(anar::NEXT_PAGE, 1 << 15);
        assert_eq!(anar::ACK, 1 << 14);
        assert_eq!(anar::REMOTE_FAULT, 1 << 13);
        assert_eq!(anar::PAUSE, 1 << 10);
        assert_eq!(anar::T4, 1 << 9);
        assert_eq!(anar::TX_FD, 1 << 8);
        assert_eq!(anar::TX_HD, 1 << 7);
        assert_eq!(anar::T10_FD, 1 << 6);
        assert_eq!(anar::T10_HD, 1 << 5);
    }

    #[test]
    fn anar_selector_field() {
        assert_eq!(anar::SELECTOR, 0x001F);
        assert_eq!(anar::SELECTOR_IEEE802_3, 0x0001);
    }

    #[test]
    fn anar_full_advertisement() {
        // Typical full advertisement
        let anar_val = anar::TX_FD | anar::TX_HD | anar::T10_FD | anar::T10_HD | anar::SELECTOR_IEEE802_3;
        assert!(anar_val & anar::TX_FD != 0);
        assert!(anar_val & anar::TX_HD != 0);
        assert!(anar_val & anar::T10_FD != 0);
        assert!(anar_val & anar::T10_HD != 0);
        assert_eq!(anar_val & anar::SELECTOR, anar::SELECTOR_IEEE802_3);
    }

    // =========================================================================
    // ANLPAR Speed/Duplex Parsing Tests
    // =========================================================================

    #[test]
    fn anlpar_100_fd_parsing() {
        // Partner advertises 100 FD
        let anlpar_val = anlpar::CAN_100_FD | anlpar::SELECTOR_802_3;

        assert!(anlpar_val & anlpar::CAN_100_FD != 0);
        assert!(anlpar_val & anlpar::CAN_100_HD == 0);
    }

    #[test]
    fn anlpar_10_hd_parsing() {
        // Partner only advertises 10 HD
        let anlpar_val = anlpar::CAN_10_HD | anlpar::SELECTOR_802_3;

        assert!(anlpar_val & anlpar::CAN_10_HD != 0);
        assert!(anlpar_val & anlpar::CAN_100_FD == 0);
        assert!(anlpar_val & anlpar::CAN_100_HD == 0);
        assert!(anlpar_val & anlpar::CAN_10_FD == 0);
    }

    #[test]
    fn anlpar_full_capability() {
        // Partner advertises everything
        let anlpar_val = anlpar::CAN_100_FD
            | anlpar::CAN_100_HD
            | anlpar::CAN_10_FD
            | anlpar::CAN_10_HD
            | anlpar::PAUSE
            | anlpar::SELECTOR_802_3;

        // Best speed/duplex should be 100 FD
        let can_100_fd = anlpar_val & anlpar::CAN_100_FD != 0;
        let can_100_hd = anlpar_val & anlpar::CAN_100_HD != 0;
        let can_10_fd = anlpar_val & anlpar::CAN_10_FD != 0;
        let can_10_hd = anlpar_val & anlpar::CAN_10_HD != 0;

        assert!(can_100_fd);
        assert!(can_100_hd);
        assert!(can_10_fd);
        assert!(can_10_hd);
    }

    #[test]
    fn anlpar_pause_capability() {
        let with_pause = anlpar::PAUSE | anlpar::SELECTOR_802_3;
        let without_pause = anlpar::SELECTOR_802_3;

        assert!(with_pause & anlpar::PAUSE != 0);
        assert!(without_pause & anlpar::PAUSE == 0);
    }

    #[test]
    fn anlpar_bit_positions() {
        assert_eq!(anlpar::NEXT_PAGE, 1 << 15);
        assert_eq!(anlpar::ACK, 1 << 14);
        assert_eq!(anlpar::REMOTE_FAULT, 1 << 13);
        assert_eq!(anlpar::PAUSE_ASYM, 1 << 11);
        assert_eq!(anlpar::PAUSE, 1 << 10);
        assert_eq!(anlpar::CAN_100_T4, 1 << 9);
        assert_eq!(anlpar::CAN_100_FD, 1 << 8);
        assert_eq!(anlpar::CAN_100_HD, 1 << 7);
        assert_eq!(anlpar::CAN_10_FD, 1 << 6);
        assert_eq!(anlpar::CAN_10_HD, 1 << 5);
    }

    #[test]
    fn anlpar_selector_field() {
        assert_eq!(anlpar::SELECTOR_MASK, 0x001F);
        assert_eq!(anlpar::SELECTOR_802_3, 0x0001);
    }

    // =========================================================================
    // PhyStatus Tests
    // =========================================================================

    #[test]
    fn phy_status_default() {
        let status = PhyStatus::default();
        assert!(!status.link_up);
        assert!(!status.an_complete);
        assert!(!status.speed_100);
        assert!(!status.full_duplex);
    }

    #[test]
    fn phy_status_all_true() {
        let status = PhyStatus {
            link_up: true,
            an_complete: true,
            speed_100: true,
            full_duplex: true,
        };
        assert!(status.link_up);
        assert!(status.an_complete);
        assert!(status.speed_100);
        assert!(status.full_duplex);
    }

    #[test]
    fn phy_status_partial() {
        // Link up but AN not complete (manual config)
        let status = PhyStatus {
            link_up: true,
            an_complete: false,
            speed_100: true,
            full_duplex: false,
        };
        assert!(status.link_up);
        assert!(!status.an_complete);
        assert!(status.speed_100);
        assert!(!status.full_duplex);
    }

    #[test]
    fn phy_status_clone() {
        let status = PhyStatus {
            link_up: true,
            an_complete: true,
            speed_100: true,
            full_duplex: true,
        };
        let cloned = status;
        assert_eq!(cloned.link_up, status.link_up);
        assert_eq!(cloned.an_complete, status.an_complete);
        assert_eq!(cloned.speed_100, status.speed_100);
        assert_eq!(cloned.full_duplex, status.full_duplex);
    }

    #[test]
    fn phy_status_10_half() {
        let status = PhyStatus {
            link_up: true,
            an_complete: true,
            speed_100: false,
            full_duplex: false,
        };
        assert!(status.link_up);
        assert!(!status.speed_100);
        assert!(!status.full_duplex);
    }

    #[test]
    fn phy_status_100_half() {
        let status = PhyStatus {
            link_up: true,
            an_complete: true,
            speed_100: true,
            full_duplex: false,
        };
        assert!(status.link_up);
        assert!(status.speed_100);
        assert!(!status.full_duplex);
    }

    #[test]
    fn phy_status_10_full() {
        let status = PhyStatus {
            link_up: true,
            an_complete: true,
            speed_100: false,
            full_duplex: true,
        };
        assert!(status.link_up);
        assert!(!status.speed_100);
        assert!(status.full_duplex);
    }
}
