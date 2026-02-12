//! IEEE 802.3 PHY Register Definitions
//!
//! This module contains the standard PHY register addresses and bit definitions
//! as specified in IEEE 802.3 Clause 22. These are internal implementation
//! details used by the MDIO controller and PHY drivers.
//!
//! # Overview
//!
//! The IEEE 802.3 standard defines 32 registers (addresses 0-31) for PHY
//! management. Registers 0-15 are standardized, while 16-31 are vendor-specific.
//!
//! # Standard Registers
//!
//! | Register | Name | Description |
//! |----------|------|-------------|
//! | 0 | BMCR | Basic Mode Control |
//! | 1 | BMSR | Basic Mode Status |
//! | 2 | PHYIDR1 | PHY Identifier 1 |
//! | 3 | PHYIDR2 | PHY Identifier 2 |
//! | 4 | ANAR | Auto-Negotiation Advertisement |
//! | 5 | ANLPAR | Link Partner Ability |
//! | 6 | ANER | Auto-Negotiation Expansion |
//! | 15 | ESTATUS | Extended Status |

// Allow unused constants - these are complete register definitions for reference
#![allow(dead_code)]

// =============================================================================
// Standard PHY Register Addresses
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

// =============================================================================
// BMCR - Basic Mode Control Register (Register 0)
// =============================================================================

/// BMCR (Basic Mode Control Register) bits
pub mod bmcr {
    /// Soft reset - self-clearing
    pub const RESET: u16 = 1 << 15;
    /// Loopback mode
    pub const LOOPBACK: u16 = 1 << 14;
    /// Speed select (100 Mbps if set, 10 Mbps if clear)
    pub const SPEED_100: u16 = 1 << 13;
    /// Auto-negotiation enable
    pub const AN_ENABLE: u16 = 1 << 12;
    /// Power down
    pub const POWER_DOWN: u16 = 1 << 11;
    /// Isolate PHY from RMII/MII
    pub const ISOLATE: u16 = 1 << 10;
    /// Restart auto-negotiation - self-clearing
    pub const AN_RESTART: u16 = 1 << 9;
    /// Duplex mode (full duplex if set)
    pub const DUPLEX_FULL: u16 = 1 << 8;
}

// =============================================================================
// BMSR - Basic Mode Status Register (Register 1)
// =============================================================================

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
    /// Remote fault detected
    pub const REMOTE_FAULT: u16 = 1 << 4;
    /// Auto-negotiation ability
    pub const AN_ABILITY: u16 = 1 << 3;
    /// Link status (1 = link up, 0 = link down)
    pub const LINK_STATUS: u16 = 1 << 2;
    /// Jabber condition detected
    pub const JABBER_DETECT: u16 = 1 << 1;
    /// Extended register capabilities
    pub const EXT_CAPABLE: u16 = 1 << 0;
}

// =============================================================================
// ANAR - Auto-Negotiation Advertisement Register (Register 4)
// =============================================================================

/// ANAR (Auto-Negotiation Advertisement Register) bits
pub mod anar {
    /// Next page indication
    pub const NEXT_PAGE: u16 = 1 << 15;
    /// Acknowledge
    pub const ACK: u16 = 1 << 14;
    /// Remote fault
    pub const REMOTE_FAULT: u16 = 1 << 13;
    /// Pause capable
    pub const PAUSE: u16 = 1 << 10;
    /// 100BASE-T4 advertised
    pub const T4: u16 = 1 << 9;
    /// 100BASE-TX full duplex advertised
    pub const TX_FD: u16 = 1 << 8;
    /// 100BASE-TX half duplex advertised
    pub const TX_HD: u16 = 1 << 7;
    /// 10BASE-T full duplex advertised
    pub const T10_FD: u16 = 1 << 6;
    /// 10BASE-T half duplex advertised
    pub const T10_HD: u16 = 1 << 5;
    /// Selector field mask
    pub const SELECTOR: u16 = 0x001F;
    /// IEEE 802.3 selector value
    pub const SELECTOR_IEEE802_3: u16 = 0x0001;
}

// =============================================================================
// ANLPAR - Auto-Negotiation Link Partner Ability Register (Register 5)
// =============================================================================

/// ANLPAR (Auto-Negotiation Link Partner Ability Register) bits
///
/// Same bit layout as ANAR, but represents what the link partner advertises.
pub mod anlpar {
    /// Next page capability
    pub const NEXT_PAGE: u16 = 1 << 15;
    /// Acknowledge received
    pub const ACK: u16 = 1 << 14;
    /// Remote fault indicated
    pub const REMOTE_FAULT: u16 = 1 << 13;
    /// Asymmetric pause
    pub const PAUSE_ASYM: u16 = 1 << 11;
    /// Pause capable
    pub const PAUSE: u16 = 1 << 10;
    /// 100BASE-T4 capable
    pub const CAN_100_T4: u16 = 1 << 9;
    /// 100BASE-TX full duplex capable
    pub const CAN_100_FD: u16 = 1 << 8;
    /// 100BASE-TX half duplex capable
    pub const CAN_100_HD: u16 = 1 << 7;
    /// 10BASE-T full duplex capable
    pub const CAN_10_FD: u16 = 1 << 6;
    /// 10BASE-T half duplex capable
    pub const CAN_10_HD: u16 = 1 << 5;
    /// Selector field mask
    pub const SELECTOR_MASK: u16 = 0x001F;
    /// IEEE 802.3 selector value
    pub const SELECTOR_802_3: u16 = 0x0001;
}

// =============================================================================
// ANER - Auto-Negotiation Expansion Register (Register 6)
// =============================================================================

/// ANER (Auto-Negotiation Expansion Register) bits
pub mod aner {
    /// Parallel detection fault
    pub const PDF: u16 = 1 << 4;
    /// Link partner next page able
    pub const LP_NEXT_PAGE_ABLE: u16 = 1 << 3;
    /// Local device next page able
    pub const NEXT_PAGE_ABLE: u16 = 1 << 2;
    /// Page received
    pub const PAGE_RX: u16 = 1 << 1;
    /// Link partner auto-negotiation able
    pub const LP_AN_ABLE: u16 = 1 << 0;
}
