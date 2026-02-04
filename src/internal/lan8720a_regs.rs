//! LAN8720A Vendor-Specific Register Definitions
//!
//! This module contains the internal register definitions for the
//! Microchip/SMSC LAN8720A 10/100 Ethernet PHY.
//!
//! # Module Organization
//!
//! - `phy_id`: PHY identifier constants
//! - `reg`: Register addresses
//! - `mcsr`: Mode Control/Status Register bits
//! - `smr`: Special Modes Register bits
//! - `scsir`: Special Control/Status Indication Register bits
//! - `isr`: Interrupt Source Register bits
//! - `pscsr`: PHY Special Control/Status Register bits
//!
//! # References
//!
//! - LAN8720A Datasheet (DS00002165)
//! - IEEE 802.3 Ethernet Standard

#![allow(dead_code)]

// =============================================================================
// LAN8720A PHY Identifier
// =============================================================================

/// PHY identifier constants
pub mod phy_id {
    /// LAN8720A PHY Identifier
    ///
    /// The PHY ID register values:
    /// - PHYIDR1 (reg 2): 0x0007
    /// - PHYIDR2 (reg 3): 0xC0Fx (x = revision)
    ///
    /// Full ID: 0x0007C0Fx
    pub const ID: u32 = 0x0007_C0F0;
    /// PHY ID mask (ignores revision bits)
    pub const MASK: u32 = 0xFFFF_FFF0;
}

// =============================================================================
// Internal Constants
// =============================================================================

/// Internal timing constants
pub mod timing {
    /// Maximum reset attempts
    pub const RESET_MAX_ATTEMPTS: u32 = 1000;
    /// Maximum auto-negotiation polling iterations
    pub const AN_MAX_ATTEMPTS: u32 = 5000;
    /// Hardware reset pulse duration in microseconds (minimum 100µs per datasheet)
    pub const RESET_PULSE_US: u32 = 200;
    /// Hardware reset recovery time in microseconds (minimum 800µs per datasheet)
    pub const RESET_RECOVERY_US: u32 = 1000;
}

// =============================================================================
// LAN8720A Vendor-Specific Registers
// =============================================================================

/// LAN8720A vendor-specific register addresses
pub mod reg {
    /// Mode Control/Status Register
    pub const MCSR: u8 = 17;
    /// Special Modes Register
    pub const SMR: u8 = 18;
    /// Symbol Error Counter Register
    pub const SECR: u8 = 26;
    /// Special Control/Status Indication Register
    pub const SCSIR: u8 = 27;
    /// Interrupt Source Register
    pub const ISR: u8 = 29;
    /// Interrupt Mask Register
    pub const IMR: u8 = 30;
    /// PHY Special Control/Status Register
    pub const PSCSR: u8 = 31;
}

/// Mode Control/Status Register (17) bits
pub mod mcsr {
    /// EDPWRDOWN - Enable Energy Detect Power Down mode
    pub const EDPWRDOWN: u16 = 1 << 13;
    /// FARLOOPBACK - Enable far loopback
    pub const FARLOOPBACK: u16 = 1 << 9;
    /// ALTINT - Alternate interrupt mode
    pub const ALTINT: u16 = 1 << 6;
    /// ENERGYON - PHY is awake (read-only)
    pub const ENERGYON: u16 = 1 << 1;
}

/// Special Modes Register (18) bits
pub mod smr {
    /// MODE mask (bits 7:5) - PHY mode selection
    pub const MODE_MASK: u16 = 0x7 << 5;
    /// Mode: 10BASE-T Half Duplex
    pub const MODE_10HD: u16 = 0x0 << 5;
    /// Mode: 10BASE-T Full Duplex
    pub const MODE_10FD: u16 = 0x1 << 5;
    /// Mode: 100BASE-TX Half Duplex
    pub const MODE_100HD: u16 = 0x2 << 5;
    /// Mode: 100BASE-TX Full Duplex
    pub const MODE_100FD: u16 = 0x3 << 5;
    /// Mode: 100BASE-TX Half Duplex (auto-neg advertised)
    pub const MODE_100HD_AN: u16 = 0x4 << 5;
    /// Mode: Repeater mode
    pub const MODE_REPEATER: u16 = 0x5 << 5;
    /// Mode: Power down
    pub const MODE_PWRDOWN: u16 = 0x6 << 5;
    /// Mode: All capable, auto-neg enabled (default)
    pub const MODE_ALL_AN: u16 = 0x7 << 5;
    /// PHYAD mask (bits 4:0) - PHY address
    pub const PHYAD_MASK: u16 = 0x1F;
}

/// Special Control/Status Indication Register (27) bits
pub mod scsir {
    /// AMDIXCTRL - Auto-MDIX control
    pub const AMDIXCTRL: u16 = 1 << 15;
    /// CH_SELECT - Manual crossover (when AMDIXCTRL=1)
    pub const CH_SELECT: u16 = 1 << 13;
    /// SQEOFF - Disable SQE test
    pub const SQEOFF: u16 = 1 << 11;
    /// XPOL - Invert polarity (10BASE-T only)
    pub const XPOL: u16 = 1 << 4;
}

/// Interrupt Source Register (29) bits
pub mod isr {
    /// ENERGYON interrupt
    pub const ENERGYON: u16 = 1 << 7;
    /// Auto-negotiation complete
    pub const AN_COMPLETE: u16 = 1 << 6;
    /// Remote fault detected
    pub const REMOTE_FAULT: u16 = 1 << 5;
    /// Link down
    pub const LINK_DOWN: u16 = 1 << 4;
    /// Auto-negotiation LP acknowledge
    pub const AN_LP_ACK: u16 = 1 << 3;
    /// Parallel detection fault
    pub const PD_FAULT: u16 = 1 << 2;
    /// Auto-negotiation page received
    pub const AN_PAGE_RX: u16 = 1 << 1;
}

/// PHY Special Control/Status Register (31) bits
pub mod pscsr {
    /// AUTODONE - Auto-negotiation done (read-only)
    pub const AUTODONE: u16 = 1 << 12;
    /// HCDSPEED mask (bits 4:2) - Speed indication
    pub const HCDSPEED_MASK: u16 = 0x7 << 2;
    /// Speed: 10BASE-T Half Duplex
    pub const HCDSPEED_10HD: u16 = 0x1 << 2;
    /// Speed: 10BASE-T Full Duplex
    pub const HCDSPEED_10FD: u16 = 0x5 << 2;
    /// Speed: 100BASE-TX Half Duplex
    pub const HCDSPEED_100HD: u16 = 0x2 << 2;
    /// Speed: 100BASE-TX Full Duplex
    pub const HCDSPEED_100FD: u16 = 0x6 << 2;
}
