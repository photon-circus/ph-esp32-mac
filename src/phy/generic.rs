//! Generic PHY Driver Trait
//!
//! This module defines the common interface for all Ethernet PHY drivers,
//! based on IEEE 802.3 Clause 22 standard registers.

use crate::config::{Duplex, Speed};
use crate::error::Result;
use crate::hal::mdio::MdioBus;

// =============================================================================
// Link Status
// =============================================================================

/// Ethernet link status information
///
/// Contains the negotiated or configured link parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LinkStatus {
    /// Link speed
    pub speed: Speed,
    /// Duplex mode
    pub duplex: Duplex,
}

impl LinkStatus {
    /// Create a new link status
    pub const fn new(speed: Speed, duplex: Duplex) -> Self {
        Self { speed, duplex }
    }

    /// 100 Mbps Full Duplex
    pub const fn fast_full() -> Self {
        Self::new(Speed::Mbps100, Duplex::Full)
    }

    /// 100 Mbps Half Duplex
    pub const fn fast_half() -> Self {
        Self::new(Speed::Mbps100, Duplex::Half)
    }

    /// 10 Mbps Full Duplex
    pub const fn slow_full() -> Self {
        Self::new(Speed::Mbps10, Duplex::Full)
    }

    /// 10 Mbps Half Duplex
    pub const fn slow_half() -> Self {
        Self::new(Speed::Mbps10, Duplex::Half)
    }
}

// =============================================================================
// PHY Capabilities
// =============================================================================

/// PHY hardware capabilities
///
/// Indicates what features the PHY chip supports.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PhyCapabilities {
    /// Supports 100BASE-TX Full Duplex
    pub speed_100_fd: bool,
    /// Supports 100BASE-TX Half Duplex
    pub speed_100_hd: bool,
    /// Supports 10BASE-T Full Duplex
    pub speed_10_fd: bool,
    /// Supports 10BASE-T Half Duplex
    pub speed_10_hd: bool,
    /// Supports auto-negotiation
    pub auto_negotiation: bool,
    /// Supports PAUSE flow control
    pub pause: bool,
    /// Supports asymmetric PAUSE
    pub pause_asymmetric: bool,
}

impl PhyCapabilities {
    /// Default 10/100 Mbps PHY capabilities
    pub const fn standard_10_100() -> Self {
        Self {
            speed_100_fd: true,
            speed_100_hd: true,
            speed_10_fd: true,
            speed_10_hd: true,
            auto_negotiation: true,
            pause: true,
            pause_asymmetric: false,
        }
    }
}

// =============================================================================
// PHY Driver Trait
// =============================================================================

/// Trait for Ethernet PHY drivers
///
/// This trait defines the common interface for all PHY drivers. Implementations
/// should handle chip-specific register access and initialization sequences.
///
/// # IEEE 802.3 Compliance
///
/// All PHY drivers must support the standard Clause 22 registers (0-15),
/// but may also use vendor-specific registers (16-31) for advanced features.
///
/// # Example Implementation
///
/// ```ignore
/// struct MyPhy {
///     addr: u8,
/// }
///
/// impl PhyDriver for MyPhy {
///     fn address(&self) -> u8 { self.addr }
///     
///     fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
///         self.soft_reset(mdio)?;
///         self.enable_auto_negotiation(mdio)
///     }
///     
///     // ... other methods
/// }
/// ```
pub trait PhyDriver {
    /// Get the PHY address (0-31)
    fn address(&self) -> u8;

    /// Initialize the PHY
    ///
    /// This should perform any chip-specific initialization sequence,
    /// typically including a soft reset and basic configuration.
    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()>;

    /// Perform a soft reset
    ///
    /// Writes to BMCR.RESET and waits for it to self-clear.
    fn soft_reset<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()>;

    /// Check if the link is up
    ///
    /// Returns `true` if PHY has detected a valid link partner.
    fn is_link_up<M: MdioBus>(&self, mdio: &mut M) -> Result<bool>;

    /// Get current link status with speed/duplex
    ///
    /// Returns `None` if link is down, `Some(LinkStatus)` if link is up.
    fn link_status<M: MdioBus>(&self, mdio: &mut M) -> Result<Option<LinkStatus>>;

    /// Poll for link changes
    ///
    /// This is a convenience method that should be called periodically.
    /// Returns `Some(LinkStatus)` when a new link is established,
    /// `None` if link is still down or unchanged.
    fn poll_link<M: MdioBus>(&mut self, mdio: &mut M) -> Result<Option<LinkStatus>>;

    /// Enable auto-negotiation
    ///
    /// Configures the PHY to automatically negotiate speed and duplex
    /// with the link partner.
    fn enable_auto_negotiation<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()>;

    /// Force specific speed and duplex
    ///
    /// Disables auto-negotiation and forces the PHY to use the specified
    /// link parameters. Use with caution - mismatched settings will cause
    /// link failure.
    fn force_link<M: MdioBus>(&mut self, mdio: &mut M, status: LinkStatus) -> Result<()>;

    /// Get PHY capabilities
    ///
    /// Returns what speed/duplex modes this PHY supports.
    fn capabilities<M: MdioBus>(&self, mdio: &mut M) -> Result<PhyCapabilities>;

    /// Read the PHY identifier (OUI + model + revision)
    ///
    /// Returns a 32-bit value: `(PHYIDR1 << 16) | PHYIDR2`
    fn phy_id<M: MdioBus>(&self, mdio: &mut M) -> Result<u32>;

    /// Check if auto-negotiation is complete
    fn is_auto_negotiation_complete<M: MdioBus>(&self, mdio: &mut M) -> Result<bool>;

    /// Get the link partner's advertised abilities (if AN complete)
    fn link_partner_abilities<M: MdioBus>(&self, mdio: &mut M) -> Result<PhyCapabilities>;
}

// =============================================================================
// Default Implementations
// =============================================================================

/// Helper functions using standard IEEE 802.3 registers
pub mod ieee802_3 {
    use super::*;
    use crate::internal::phy_registers::{anar, bmcr, bmsr, phy_reg};

    /// Read BMSR and check link status bit
    pub fn is_link_up<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<bool> {
        let bmsr_val = mdio.read(phy_addr, phy_reg::BMSR)?;
        Ok((bmsr_val & bmsr::LINK_STATUS) != 0)
    }

    /// Read BMSR and check AN complete bit
    pub fn is_an_complete<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<bool> {
        let bmsr_val = mdio.read(phy_addr, phy_reg::BMSR)?;
        Ok((bmsr_val & bmsr::AN_COMPLETE) != 0)
    }

    /// Perform soft reset via BMCR
    pub fn soft_reset<M: MdioBus>(mdio: &mut M, phy_addr: u8, max_attempts: u32) -> Result<()> {
        // Set reset bit
        mdio.write(phy_addr, phy_reg::BMCR, bmcr::RESET)?;

        // Wait for reset to complete (bit self-clears)
        for _ in 0..max_attempts {
            let bmcr_val = mdio.read(phy_addr, phy_reg::BMCR)?;
            if (bmcr_val & bmcr::RESET) == 0 {
                return Ok(());
            }
        }

        // If we get here, reset didn't complete - but don't fail
        // Some PHYs may be slow to clear the bit
        Ok(())
    }

    /// Enable auto-negotiation and restart
    pub fn enable_auto_negotiation<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<()> {
        let bmcr_val = mdio.read(phy_addr, phy_reg::BMCR)?;
        mdio.write(
            phy_addr,
            phy_reg::BMCR,
            (bmcr_val | bmcr::AN_ENABLE | bmcr::AN_RESTART) & !bmcr::ISOLATE,
        )
    }

    /// Force speed and duplex
    pub fn force_link<M: MdioBus>(mdio: &mut M, phy_addr: u8, status: LinkStatus) -> Result<()> {
        let mut bmcr_val = mdio.read(phy_addr, phy_reg::BMCR)?;

        // Disable auto-negotiation
        bmcr_val &= !bmcr::AN_ENABLE;
        bmcr_val &= !bmcr::ISOLATE;

        // Set speed
        if matches!(status.speed, Speed::Mbps100) {
            bmcr_val |= bmcr::SPEED_100;
        } else {
            bmcr_val &= !bmcr::SPEED_100;
        }

        // Set duplex
        if matches!(status.duplex, Duplex::Full) {
            bmcr_val |= bmcr::DUPLEX_FULL;
        } else {
            bmcr_val &= !bmcr::DUPLEX_FULL;
        }

        mdio.write(phy_addr, phy_reg::BMCR, bmcr_val)
    }

    /// Read PHY ID from PHYIDR1 and PHYIDR2
    pub fn read_phy_id<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<u32> {
        let id1 = mdio.read(phy_addr, phy_reg::PHYIDR1)? as u32;
        let id2 = mdio.read(phy_addr, phy_reg::PHYIDR2)? as u32;
        Ok((id1 << 16) | id2)
    }

    /// Read capabilities from BMSR
    pub fn read_capabilities<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<PhyCapabilities> {
        let bmsr_val = mdio.read(phy_addr, phy_reg::BMSR)?;

        Ok(PhyCapabilities {
            speed_100_fd: (bmsr_val & bmsr::TX_FD_CAPABLE) != 0,
            speed_100_hd: (bmsr_val & bmsr::TX_HD_CAPABLE) != 0,
            speed_10_fd: (bmsr_val & bmsr::T10_FD_CAPABLE) != 0,
            speed_10_hd: (bmsr_val & bmsr::T10_HD_CAPABLE) != 0,
            auto_negotiation: (bmsr_val & bmsr::AN_ABILITY) != 0,
            pause: false, // Need to check ANAR for this
            pause_asymmetric: false,
        })
    }

    /// Read link partner abilities from ANLPAR
    pub fn read_link_partner<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<PhyCapabilities> {
        let anlpar_val = mdio.read(phy_addr, phy_reg::ANLPAR)?;

        Ok(PhyCapabilities {
            speed_100_fd: (anlpar_val & anar::TX_FD) != 0,
            speed_100_hd: (anlpar_val & anar::TX_HD) != 0,
            speed_10_fd: (anlpar_val & anar::T10_FD) != 0,
            speed_10_hd: (anlpar_val & anar::T10_HD) != 0,
            auto_negotiation: true, // If we have ANLPAR, partner supports AN
            pause: (anlpar_val & anar::PAUSE) != 0,
            pause_asymmetric: false,
        })
    }

    /// Get link status from BMCR (when AN is disabled or for current state)
    pub fn link_status_from_bmcr<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<LinkStatus> {
        let bmcr_val = mdio.read(phy_addr, phy_reg::BMCR)?;

        let speed = if (bmcr_val & bmcr::SPEED_100) != 0 {
            Speed::Mbps100
        } else {
            Speed::Mbps10
        };

        let duplex = if (bmcr_val & bmcr::DUPLEX_FULL) != 0 {
            Duplex::Full
        } else {
            Duplex::Half
        };

        Ok(LinkStatus::new(speed, duplex))
    }
}
