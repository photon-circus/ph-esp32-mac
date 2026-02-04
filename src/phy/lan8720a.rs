//! LAN8720A PHY Driver
//!
//! Driver for the Microchip/SMSC LAN8720A 10/100 Ethernet PHY.
//!
//! The LAN8720A is the most commonly used PHY with ESP32 due to its:
//! - Low cost and wide availability
//! - RMII interface support
//! - Flexible reference clock options
//! - Small package (QFN24)
//!
//! # Wiring with ESP32 (RMII Mode)
//!
//! | LAN8720A Pin | ESP32 GPIO | Function |
//! |--------------|------------|----------|
//! | MDC          | GPIO23     | SMI Clock |
//! | MDIO         | GPIO18     | SMI Data |
//! | TX_EN        | GPIO21     | TX Enable |
//! | TXD0         | GPIO19     | TX Data 0 |
//! | TXD1         | GPIO22     | TX Data 1 |
//! | CRS_DV       | GPIO27     | Carrier Sense / RX Data Valid |
//! | RXD0         | GPIO25     | RX Data 0 |
//! | RXD1         | GPIO26     | RX Data 1 |
//! | nINT/REFCLK  | GPIO0      | 50 MHz Reference Clock |
//! | nRST         | Any GPIO   | Reset (active low, optional) |
//!
//! # Reset Pin
//!
//! The LAN8720A has an active-low reset pin (nRST). While soft reset via
//! MDIO is usually sufficient, hardware reset provides a more reliable
//! reset when the PHY is in an unknown state.
//!
//! The driver supports an optional reset pin using `embedded_hal::digital::OutputPin`:
//!
//! ```ignore
//! use esp32_emac::phy::{Lan8720aWithReset, PhyDriver};
//!
//! // With esp-hal GPIO
//! let reset_pin = io.pins.gpio5.into_push_pull_output();
//! let mut phy = Lan8720aWithReset::new(0, reset_pin);
//! phy.hardware_reset(&mut delay)?;
//! phy.init(&mut mdio)?;
//! ```
//!
//! # PHY Address
//!
//! The LAN8720A PHY address is configurable via the PHYAD0 pin:
//! - PHYAD0 = LOW: Address 0
//! - PHYAD0 = HIGH: Address 1
//!
//! Most modules have PHYAD0 tied to GND, so address 0 is typical.
//!
//! # Reference Clock
//!
//! The LAN8720A can accept a 50 MHz reference clock from:
//! - External crystal (XI/XO pins)
//! - External clock input on nINT/REFCLK pin (most common with ESP32)
//!
//! When using ESP32's internal clock output on GPIO0/16/17, configure
//! `RmiiClockMode::InternalOutput`.
//!
//! # Example
//!
//! ```ignore
//! use esp32_emac::phy::{Lan8720a, PhyDriver};
//!
//! let mut phy = Lan8720a::new(0);  // Address 0
//! phy.init(&mut mdio)?;
//!
//! // Wait for link
//! loop {
//!     if let Some(link) = phy.poll_link(&mut mdio)? {
//!         emac.set_speed(link.speed);
//!         emac.set_duplex(link.duplex);
//!         break;
//!     }
//!     // delay...
//! }
//! ```

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;

use crate::error::Result;
use crate::hal::mdio::MdioBus;

use super::generic::{LinkStatus, PhyCapabilities, PhyDriver, ieee802_3};

// =============================================================================
// LAN8720A Constants
// =============================================================================

/// LAN8720A PHY Identifier
///
/// The PHY ID register values:
/// - PHYIDR1 (reg 2): 0x0007
/// - PHYIDR2 (reg 3): 0xC0Fx (x = revision)
///
/// Full ID: 0x0007C0Fx
pub const LAN8720A_PHY_ID: u32 = 0x0007_C0F0;
/// PHY ID mask (ignores revision bits)
pub const LAN8720A_PHY_ID_MASK: u32 = 0xFFFF_FFF0;

/// Maximum reset attempts
const RESET_MAX_ATTEMPTS: u32 = 1000;

/// Maximum auto-negotiation polling iterations
const AN_MAX_ATTEMPTS: u32 = 5000;

/// Hardware reset pulse duration in microseconds (minimum 100µs per datasheet)
const RESET_PULSE_US: u32 = 200;

/// Hardware reset recovery time in microseconds (minimum 800µs per datasheet)
const RESET_RECOVERY_US: u32 = 1000;

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

// =============================================================================
// LAN8720A Driver (without reset pin)
// =============================================================================

/// LAN8720A PHY Driver
///
/// This driver supports the Microchip/SMSC LAN8720A 10/100 Ethernet PHY
/// with RMII interface.
///
/// This variant does not include a hardware reset pin. Use [`Lan8720aWithReset`]
/// if you need hardware reset capability.
#[derive(Debug)]
pub struct Lan8720a {
    /// PHY address (0-31)
    addr: u8,
    /// Last known link state
    last_link_up: bool,
}

impl Lan8720a {
    /// Create a new LAN8720A driver
    ///
    /// # Arguments
    /// * `addr` - PHY address (typically 0 or 1)
    pub const fn new(addr: u8) -> Self {
        Self {
            addr,
            last_link_up: false,
        }
    }

    /// Verify this is a LAN8720A by reading the PHY ID
    pub fn verify_id<M: MdioBus>(&self, mdio: &mut M) -> Result<bool> {
        let id = ieee802_3::read_phy_id(mdio, self.addr)?;
        Ok((id & LAN8720A_PHY_ID_MASK) == LAN8720A_PHY_ID)
    }

    /// Get the revision number from PHY ID
    pub fn revision<M: MdioBus>(&self, mdio: &mut M) -> Result<u8> {
        let id = ieee802_3::read_phy_id(mdio, self.addr)?;
        Ok((id & 0x0F) as u8)
    }

    /// Read the speed/duplex indication from vendor-specific register
    ///
    /// This is more reliable than reading BMCR after auto-negotiation
    /// because it shows the actual negotiated result.
    pub fn read_speed_indication<M: MdioBus>(&self, mdio: &mut M) -> Result<Option<LinkStatus>> {
        let pscsr = mdio.read(self.addr, reg::PSCSR)?;

        // Check if auto-negotiation is done
        if (pscsr & pscsr::AUTODONE) == 0 {
            return Ok(None);
        }

        let speed_bits = pscsr & pscsr::HCDSPEED_MASK;
        let link = match speed_bits {
            x if x == pscsr::HCDSPEED_100FD => LinkStatus::fast_full(),
            x if x == pscsr::HCDSPEED_100HD => LinkStatus::fast_half(),
            x if x == pscsr::HCDSPEED_10FD => LinkStatus::slow_full(),
            x if x == pscsr::HCDSPEED_10HD => LinkStatus::slow_half(),
            _ => return Ok(None), // Unknown or invalid
        };

        Ok(Some(link))
    }

    /// Enable or disable Energy Detect Power Down mode
    ///
    /// When enabled, the PHY will enter a low-power state when no link
    /// activity is detected.
    pub fn set_energy_detect_powerdown<M: MdioBus>(
        &mut self,
        mdio: &mut M,
        enabled: bool,
    ) -> Result<()> {
        let mut mcsr = mdio.read(self.addr, reg::MCSR)?;
        if enabled {
            mcsr |= mcsr::EDPWRDOWN;
        } else {
            mcsr &= !mcsr::EDPWRDOWN;
        }
        mdio.write(self.addr, reg::MCSR, mcsr)
    }

    /// Check if PHY is awake (has energy on cable)
    pub fn is_energy_on<M: MdioBus>(&self, mdio: &mut M) -> Result<bool> {
        let mcsr = mdio.read(self.addr, reg::MCSR)?;
        Ok((mcsr & mcsr::ENERGYON) != 0)
    }

    /// Read interrupt status (clears on read)
    pub fn read_interrupt_status<M: MdioBus>(&self, mdio: &mut M) -> Result<u16> {
        mdio.read(self.addr, reg::ISR)
    }

    /// Enable specific interrupts
    pub fn set_interrupt_mask<M: MdioBus>(&mut self, mdio: &mut M, mask: u16) -> Result<()> {
        mdio.write(self.addr, reg::IMR, mask)
    }

    /// Enable link change interrupt
    pub fn enable_link_interrupt<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        let mask = isr::LINK_DOWN | isr::AN_COMPLETE;
        self.set_interrupt_mask(mdio, mask)
    }

    /// Read symbol error counter
    pub fn symbol_error_count<M: MdioBus>(&self, mdio: &mut M) -> Result<u16> {
        mdio.read(self.addr, reg::SECR)
    }

    /// Configure advertisement for auto-negotiation
    ///
    /// # Arguments
    /// * `caps` - Capabilities to advertise
    pub fn configure_advertisement<M: MdioBus>(
        &mut self,
        mdio: &mut M,
        caps: &PhyCapabilities,
    ) -> Result<()> {
        use crate::hal::mdio::{anar, phy_reg};

        let mut anar_val = anar::SELECTOR_IEEE802_3;

        if caps.speed_100_fd {
            anar_val |= anar::TX_FD;
        }
        if caps.speed_100_hd {
            anar_val |= anar::TX_HD;
        }
        if caps.speed_10_fd {
            anar_val |= anar::T10_FD;
        }
        if caps.speed_10_hd {
            anar_val |= anar::T10_HD;
        }
        if caps.pause {
            anar_val |= anar::PAUSE;
        }

        mdio.write(self.addr, phy_reg::ANAR, anar_val)
    }
}

impl PhyDriver for Lan8720a {
    fn address(&self) -> u8 {
        self.addr
    }

    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        // Perform soft reset
        self.soft_reset(mdio)?;

        // Disable energy detect power down (can cause issues during development)
        self.set_energy_detect_powerdown(mdio, false)?;

        // Enable auto-negotiation with all capabilities
        self.enable_auto_negotiation(mdio)?;

        self.last_link_up = false;
        Ok(())
    }

    fn soft_reset<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        ieee802_3::soft_reset(mdio, self.addr, RESET_MAX_ATTEMPTS)
    }

    fn is_link_up<M: MdioBus>(&self, mdio: &mut M) -> Result<bool> {
        ieee802_3::is_link_up(mdio, self.addr)
    }

    fn link_status<M: MdioBus>(&self, mdio: &mut M) -> Result<Option<LinkStatus>> {
        if !self.is_link_up(mdio)? {
            return Ok(None);
        }

        // Use vendor-specific register for accurate speed/duplex
        self.read_speed_indication(mdio)
    }

    fn poll_link<M: MdioBus>(&mut self, mdio: &mut M) -> Result<Option<LinkStatus>> {
        let link_up = self.is_link_up(mdio)?;

        if link_up && !self.last_link_up {
            // Link just came up - get status
            self.last_link_up = true;
            return self.read_speed_indication(mdio);
        }

        if !link_up && self.last_link_up {
            // Link just went down
            self.last_link_up = false;
        }

        Ok(None)
    }

    fn enable_auto_negotiation<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        // Advertise all capabilities
        let caps = PhyCapabilities::standard_10_100();
        self.configure_advertisement(mdio, &caps)?;

        // Enable and restart AN
        ieee802_3::enable_auto_negotiation(mdio, self.addr)
    }

    fn force_link<M: MdioBus>(&mut self, mdio: &mut M, status: LinkStatus) -> Result<()> {
        ieee802_3::force_link(mdio, self.addr, status)
    }

    fn capabilities<M: MdioBus>(&self, mdio: &mut M) -> Result<PhyCapabilities> {
        ieee802_3::read_capabilities(mdio, self.addr)
    }

    fn phy_id<M: MdioBus>(&self, mdio: &mut M) -> Result<u32> {
        ieee802_3::read_phy_id(mdio, self.addr)
    }

    fn is_auto_negotiation_complete<M: MdioBus>(&self, mdio: &mut M) -> Result<bool> {
        ieee802_3::is_an_complete(mdio, self.addr)
    }

    fn link_partner_abilities<M: MdioBus>(&self, mdio: &mut M) -> Result<PhyCapabilities> {
        ieee802_3::read_link_partner(mdio, self.addr)
    }
}

// =============================================================================
// LAN8720A Driver (with reset pin)
// =============================================================================

/// LAN8720A PHY Driver with Hardware Reset Pin
///
/// This variant of the LAN8720A driver includes support for hardware reset
/// via an `embedded_hal::digital::OutputPin`. The reset pin is active-low.
///
/// # Example
///
/// ```ignore
/// use esp32_emac::phy::{Lan8720aWithReset, PhyDriver};
/// use embedded_hal::delay::DelayNs;
///
/// // Create PHY driver with reset pin
/// let reset_pin = io.pins.gpio5.into_push_pull_output();
/// let mut phy = Lan8720aWithReset::new(0, reset_pin);
///
/// // Perform hardware reset before initialization
/// phy.hardware_reset(&mut delay)?;
/// phy.init(&mut mdio)?;
/// ```
#[derive(Debug)]
pub struct Lan8720aWithReset<RST: OutputPin> {
    /// Inner PHY driver
    inner: Lan8720a,
    /// Reset pin (active low)
    reset_pin: RST,
}

impl<RST: OutputPin> Lan8720aWithReset<RST> {
    /// Create a new LAN8720A driver with reset pin
    ///
    /// The reset pin should be configured as a push-pull output.
    /// The pin will be set high (inactive) initially.
    ///
    /// # Arguments
    /// * `addr` - PHY address (typically 0 or 1)
    /// * `reset_pin` - Reset pin implementing `OutputPin` (active low)
    pub fn new(addr: u8, mut reset_pin: RST) -> Self {
        // Ensure reset is inactive (high)
        let _ = reset_pin.set_high();
        Self {
            inner: Lan8720a::new(addr),
            reset_pin,
        }
    }

    /// Perform hardware reset of the PHY
    ///
    /// This pulses the reset pin low, then waits for the PHY to recover.
    /// Call this before `init()` if the PHY might be in an unknown state.
    ///
    /// # Timing
    /// - Reset pulse: 200µs (minimum 100µs per datasheet)
    /// - Recovery time: 1ms (minimum 800µs per datasheet)
    pub fn hardware_reset<D: DelayNs>(&mut self, delay: &mut D) -> Result<()> {
        // Assert reset (low)
        self.reset_pin
            .set_low()
            .map_err(|_| crate::error::ConfigError::GpioError)?;
        delay.delay_us(RESET_PULSE_US);

        // Deassert reset (high)
        self.reset_pin
            .set_high()
            .map_err(|_| crate::error::ConfigError::GpioError)?;
        delay.delay_us(RESET_RECOVERY_US);

        Ok(())
    }

    /// Assert reset (hold PHY in reset state)
    ///
    /// The PHY will remain in reset until `deassert_reset()` is called.
    pub fn assert_reset(&mut self) -> Result<()> {
        self.reset_pin
            .set_low()
            .map_err(|_| crate::error::ConfigError::GpioError)?;
        Ok(())
    }

    /// Deassert reset (release PHY from reset)
    ///
    /// Call this after `assert_reset()` to release the PHY.
    /// Wait at least 1ms after this before accessing the PHY via MDIO.
    pub fn deassert_reset(&mut self) -> Result<()> {
        self.reset_pin
            .set_high()
            .map_err(|_| crate::error::ConfigError::GpioError)?;
        Ok(())
    }

    /// Get mutable access to the reset pin
    pub fn reset_pin_mut(&mut self) -> &mut RST {
        &mut self.reset_pin
    }

    /// Consume the driver and return the reset pin
    pub fn into_reset_pin(self) -> RST {
        self.reset_pin
    }

    // Forward all inner methods

    /// Verify this is a LAN8720A by reading the PHY ID
    pub fn verify_id<M: MdioBus>(&self, mdio: &mut M) -> Result<bool> {
        self.inner.verify_id(mdio)
    }

    /// Get the revision number from PHY ID
    pub fn revision<M: MdioBus>(&self, mdio: &mut M) -> Result<u8> {
        self.inner.revision(mdio)
    }

    /// Read the speed/duplex indication from vendor-specific register
    pub fn read_speed_indication<M: MdioBus>(&self, mdio: &mut M) -> Result<Option<LinkStatus>> {
        self.inner.read_speed_indication(mdio)
    }

    /// Enable or disable Energy Detect Power Down mode
    pub fn set_energy_detect_powerdown<M: MdioBus>(
        &mut self,
        mdio: &mut M,
        enabled: bool,
    ) -> Result<()> {
        self.inner.set_energy_detect_powerdown(mdio, enabled)
    }

    /// Check if PHY is awake (has energy on cable)
    pub fn is_energy_on<M: MdioBus>(&self, mdio: &mut M) -> Result<bool> {
        self.inner.is_energy_on(mdio)
    }

    /// Read interrupt status (clears on read)
    pub fn read_interrupt_status<M: MdioBus>(&self, mdio: &mut M) -> Result<u16> {
        self.inner.read_interrupt_status(mdio)
    }

    /// Enable specific interrupts
    pub fn set_interrupt_mask<M: MdioBus>(&mut self, mdio: &mut M, mask: u16) -> Result<()> {
        self.inner.set_interrupt_mask(mdio, mask)
    }

    /// Enable link change interrupt
    pub fn enable_link_interrupt<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        self.inner.enable_link_interrupt(mdio)
    }

    /// Read symbol error counter
    pub fn symbol_error_count<M: MdioBus>(&self, mdio: &mut M) -> Result<u16> {
        self.inner.symbol_error_count(mdio)
    }

    /// Configure advertisement for auto-negotiation
    pub fn configure_advertisement<M: MdioBus>(
        &mut self,
        mdio: &mut M,
        caps: &PhyCapabilities,
    ) -> Result<()> {
        self.inner.configure_advertisement(mdio, caps)
    }
}

impl<RST: OutputPin> PhyDriver for Lan8720aWithReset<RST> {
    fn address(&self) -> u8 {
        self.inner.address()
    }

    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        self.inner.init(mdio)
    }

    fn soft_reset<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        self.inner.soft_reset(mdio)
    }

    fn is_link_up<M: MdioBus>(&self, mdio: &mut M) -> Result<bool> {
        self.inner.is_link_up(mdio)
    }

    fn link_status<M: MdioBus>(&self, mdio: &mut M) -> Result<Option<LinkStatus>> {
        self.inner.link_status(mdio)
    }

    fn poll_link<M: MdioBus>(&mut self, mdio: &mut M) -> Result<Option<LinkStatus>> {
        self.inner.poll_link(mdio)
    }

    fn enable_auto_negotiation<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        self.inner.enable_auto_negotiation(mdio)
    }

    fn force_link<M: MdioBus>(&mut self, mdio: &mut M, status: LinkStatus) -> Result<()> {
        self.inner.force_link(mdio, status)
    }

    fn capabilities<M: MdioBus>(&self, mdio: &mut M) -> Result<PhyCapabilities> {
        self.inner.capabilities(mdio)
    }

    fn phy_id<M: MdioBus>(&self, mdio: &mut M) -> Result<u32> {
        self.inner.phy_id(mdio)
    }

    fn is_auto_negotiation_complete<M: MdioBus>(&self, mdio: &mut M) -> Result<bool> {
        self.inner.is_auto_negotiation_complete(mdio)
    }

    fn link_partner_abilities<M: MdioBus>(&self, mdio: &mut M) -> Result<PhyCapabilities> {
        self.inner.link_partner_abilities(mdio)
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Wait for auto-negotiation to complete
///
/// This is a blocking function that polls until AN completes or times out.
pub fn wait_for_link<M: MdioBus>(phy: &mut Lan8720a, mdio: &mut M) -> Result<Option<LinkStatus>> {
    for _ in 0..AN_MAX_ATTEMPTS {
        if let Some(link) = phy.poll_link(mdio)? {
            return Ok(Some(link));
        }
        core::hint::spin_loop();
    }
    Ok(None)
}

/// Scan the MDIO bus for LAN8720A PHYs
///
/// Returns a list of addresses where LAN8720A PHYs are found.
pub fn scan_bus<M: MdioBus>(mdio: &mut M) -> Result<[Option<u8>; 32]> {
    let mut found = [None; 32];

    for addr in 0..32 {
        let phy = Lan8720a::new(addr);
        if phy.verify_id(mdio).unwrap_or(false) {
            found[addr as usize] = Some(addr);
        }
    }

    Ok(found)
}

#[cfg(test)]
#[allow(clippy::std_instead_of_alloc)]
mod tests {
    extern crate std;

    use super::*;
    use crate::config::{Duplex, Speed};
    use crate::hal::mdio::{bmcr, phy_reg};
    use crate::test_utils::MockMdioBus;
    use std::vec::Vec;

    // =========================================================================
    // PHY ID and Constants Tests
    // =========================================================================

    #[test]
    fn test_phy_id_check() {
        // LAN8720A ID should match
        assert!((0x0007_C0F0 & LAN8720A_PHY_ID_MASK) == LAN8720A_PHY_ID);
        assert!((0x0007_C0F1 & LAN8720A_PHY_ID_MASK) == LAN8720A_PHY_ID); // Different revision
        assert!((0x0007_C0FF & LAN8720A_PHY_ID_MASK) == LAN8720A_PHY_ID);

        // Other PHYs should not match
        assert!((0x0022_1555 & LAN8720A_PHY_ID_MASK) != LAN8720A_PHY_ID); // IP101
        assert!(LAN8720A_PHY_ID_MASK != LAN8720A_PHY_ID); // All 1s
        assert!((0x0001_0000 & LAN8720A_PHY_ID_MASK) != LAN8720A_PHY_ID); // Different OUI
    }

    #[test]
    fn test_speed_indication() {
        // Test HCDSPEED bit patterns
        assert_eq!(pscsr::HCDSPEED_10HD, 0x04);
        assert_eq!(pscsr::HCDSPEED_10FD, 0x14);
        assert_eq!(pscsr::HCDSPEED_100HD, 0x08);
        assert_eq!(pscsr::HCDSPEED_100FD, 0x18);
    }

    // =========================================================================
    // Initialization State Machine Tests
    // =========================================================================

    #[test]
    fn test_init_performs_soft_reset() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        // Reset bit self-clears immediately
        mdio.set_register(0, phy_reg::BMCR, 0x0000);

        let mut phy = Lan8720a::new(0);
        phy.init(&mut mdio).unwrap();

        // Check that BMCR.RESET was written
        let writes = mdio.get_writes();
        let reset_writes: Vec<_> = writes
            .iter()
            .filter(|(addr, reg, val)| {
                *addr == 0 && *reg == phy_reg::BMCR && (*val & bmcr::RESET) != 0
            })
            .collect();
        assert!(!reset_writes.is_empty(), "Expected BMCR.RESET write");
    }

    #[test]
    fn test_init_disables_energy_detect_powerdown() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::MCSR, mcsr::EDPWRDOWN); // Start with EDPWRDOWN enabled

        let mut phy = Lan8720a::new(0);
        phy.init(&mut mdio).unwrap();

        // EDPWRDOWN should be cleared
        let mcsr_val = mdio.get_register(0, reg::MCSR).unwrap();
        assert_eq!(
            mcsr_val & mcsr::EDPWRDOWN,
            0,
            "EDPWRDOWN should be disabled"
        );
    }

    #[test]
    fn test_init_enables_auto_negotiation() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        phy.init(&mut mdio).unwrap();

        // Check BMCR has AN_ENABLE set
        let bmcr_val = mdio.get_register(0, phy_reg::BMCR).unwrap();
        assert!(bmcr_val & bmcr::AN_ENABLE != 0, "AN_ENABLE should be set");
    }

    #[test]
    fn test_init_resets_link_state() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.simulate_link_up_100_fd(0);

        let mut phy = Lan8720a::new(0);
        // Manually set internal state as if link was up
        phy.last_link_up = true;

        phy.init(&mut mdio).unwrap();

        // After init, internal state should be reset
        assert!(!phy.last_link_up, "last_link_up should be reset after init");
    }

    // =========================================================================
    // Soft Reset State Machine Tests
    // =========================================================================

    #[test]
    fn test_soft_reset_writes_reset_bit() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        // Simulate reset completing immediately
        mdio.set_register(0, phy_reg::BMCR, 0x0000);

        let mut phy = Lan8720a::new(0);
        phy.soft_reset(&mut mdio).unwrap();

        let writes = mdio.get_writes();
        let first_write = writes.first().unwrap();
        assert_eq!(first_write.0, 0);
        assert_eq!(first_write.1, phy_reg::BMCR);
        assert!(first_write.2 & bmcr::RESET != 0, "Should write RESET bit");
    }

    #[test]
    fn test_soft_reset_waits_for_reset_clear() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        // Reset bit stays set (simulates slow reset)
        mdio.set_register(0, phy_reg::BMCR, bmcr::RESET);

        let mut phy = Lan8720a::new(0);
        // This should poll multiple times and eventually return
        // (implementation doesn't fail even if reset doesn't clear)
        phy.soft_reset(&mut mdio).unwrap();
    }

    // =========================================================================
    // Link Status State Machine Tests
    // =========================================================================

    #[test]
    fn test_is_link_up_when_link_down() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        // Link is down by default

        let phy = Lan8720a::new(0);
        assert!(!phy.is_link_up(&mut mdio).unwrap());
    }

    #[test]
    fn test_is_link_up_when_link_up() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.simulate_link_up_100_fd(0);

        let phy = Lan8720a::new(0);
        assert!(phy.is_link_up(&mut mdio).unwrap());
    }

    #[test]
    fn test_link_status_returns_none_when_down() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let phy = Lan8720a::new(0);
        assert!(phy.link_status(&mut mdio).unwrap().is_none());
    }

    #[test]
    fn test_link_status_returns_speed_duplex_when_up() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.simulate_link_up_100_fd(0);
        // Set PSCSR for speed indication
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_100FD);

        let phy = Lan8720a::new(0);
        let status = phy.link_status(&mut mdio).unwrap().unwrap();
        assert_eq!(status.speed, Speed::Mbps100);
        assert_eq!(status.duplex, Duplex::Full);
    }

    // =========================================================================
    // Poll Link State Machine Tests
    // =========================================================================

    #[test]
    fn test_poll_link_returns_none_when_link_stays_down() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        assert!(phy.poll_link(&mut mdio).unwrap().is_none());
        assert!(phy.poll_link(&mut mdio).unwrap().is_none());
    }

    #[test]
    fn test_poll_link_returns_status_on_link_up_transition() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        // Set PSCSR for speed indication
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_100FD);

        let mut phy = Lan8720a::new(0);

        // First poll: link down
        assert!(phy.poll_link(&mut mdio).unwrap().is_none());

        // Link comes up
        mdio.simulate_link_up_100_fd(0);

        // Second poll: should return status (transition detected)
        let status = phy.poll_link(&mut mdio).unwrap();
        assert!(status.is_some());
        let link = status.unwrap();
        assert_eq!(link.speed, Speed::Mbps100);
        assert_eq!(link.duplex, Duplex::Full);
    }

    #[test]
    fn test_poll_link_returns_none_when_link_stays_up() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.simulate_link_up_100_fd(0);
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_100FD);

        let mut phy = Lan8720a::new(0);

        // First poll: link up (transition from unknown)
        let first = phy.poll_link(&mut mdio).unwrap();
        assert!(first.is_some());

        // Second poll: link still up (no transition)
        let second = phy.poll_link(&mut mdio).unwrap();
        assert!(second.is_none());
    }

    #[test]
    fn test_poll_link_tracks_link_down_transition() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.simulate_link_up_100_fd(0);
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_100FD);

        let mut phy = Lan8720a::new(0);

        // First poll: link up
        let _ = phy.poll_link(&mut mdio).unwrap();

        // Link goes down
        mdio.simulate_link_down(0);

        // Poll detects link down (returns None, but updates internal state)
        assert!(phy.poll_link(&mut mdio).unwrap().is_none());
        assert!(!phy.last_link_up);
    }

    #[test]
    fn test_poll_link_detects_link_flap() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_100FD);

        let mut phy = Lan8720a::new(0);

        // Link up
        mdio.simulate_link_up_100_fd(0);
        assert!(phy.poll_link(&mut mdio).unwrap().is_some());

        // Link down
        mdio.simulate_link_down(0);
        assert!(phy.poll_link(&mut mdio).unwrap().is_none());

        // Link up again - should detect transition
        mdio.simulate_link_up_100_fd(0);
        assert!(phy.poll_link(&mut mdio).unwrap().is_some());
    }

    // =========================================================================
    // Auto-Negotiation State Machine Tests
    // =========================================================================

    #[test]
    fn test_enable_auto_negotiation_writes_anar() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        phy.enable_auto_negotiation(&mut mdio).unwrap();

        // Check ANAR was written
        let writes = mdio.get_writes();
        let anar_writes: Vec<_> = writes
            .iter()
            .filter(|(_, reg, _)| *reg == phy_reg::ANAR)
            .collect();
        assert!(!anar_writes.is_empty(), "Expected ANAR write");
    }

    #[test]
    fn test_enable_auto_negotiation_restarts_an() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        phy.enable_auto_negotiation(&mut mdio).unwrap();

        // Check BMCR has AN_ENABLE and AN_RESTART
        let bmcr_val = mdio.get_register(0, phy_reg::BMCR).unwrap();
        assert!(bmcr_val & bmcr::AN_ENABLE != 0, "AN_ENABLE should be set");
        assert!(bmcr_val & bmcr::AN_RESTART != 0, "AN_RESTART should be set");
    }

    #[test]
    fn test_is_auto_negotiation_complete_when_not_done() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        // AN not complete (link down)

        let phy = Lan8720a::new(0);
        assert!(!phy.is_auto_negotiation_complete(&mut mdio).unwrap());
    }

    #[test]
    fn test_is_auto_negotiation_complete_when_done() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.simulate_link_up_100_fd(0);

        let phy = Lan8720a::new(0);
        assert!(phy.is_auto_negotiation_complete(&mut mdio).unwrap());
    }

    // =========================================================================
    // Force Link State Machine Tests
    // =========================================================================

    #[test]
    fn test_force_link_disables_auto_negotiation() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        // Start with AN enabled
        mdio.set_register(0, phy_reg::BMCR, bmcr::AN_ENABLE);

        let mut phy = Lan8720a::new(0);
        phy.force_link(&mut mdio, LinkStatus::fast_full()).unwrap();

        let bmcr_val = mdio.get_register(0, phy_reg::BMCR).unwrap();
        assert_eq!(bmcr_val & bmcr::AN_ENABLE, 0, "AN should be disabled");
    }

    #[test]
    fn test_force_link_100_full() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        phy.force_link(&mut mdio, LinkStatus::fast_full()).unwrap();

        let bmcr_val = mdio.get_register(0, phy_reg::BMCR).unwrap();
        assert!(bmcr_val & bmcr::SPEED_100 != 0, "SPEED_100 should be set");
        assert!(
            bmcr_val & bmcr::DUPLEX_FULL != 0,
            "DUPLEX_FULL should be set"
        );
    }

    #[test]
    fn test_force_link_10_half() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        phy.force_link(&mut mdio, LinkStatus::slow_half()).unwrap();

        let bmcr_val = mdio.get_register(0, phy_reg::BMCR).unwrap();
        assert_eq!(bmcr_val & bmcr::SPEED_100, 0, "SPEED_100 should be clear");
        assert_eq!(
            bmcr_val & bmcr::DUPLEX_FULL,
            0,
            "DUPLEX_FULL should be clear"
        );
    }

    #[test]
    fn test_force_link_100_half() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        phy.force_link(&mut mdio, LinkStatus::fast_half()).unwrap();

        let bmcr_val = mdio.get_register(0, phy_reg::BMCR).unwrap();
        assert!(bmcr_val & bmcr::SPEED_100 != 0, "SPEED_100 should be set");
        assert_eq!(
            bmcr_val & bmcr::DUPLEX_FULL,
            0,
            "DUPLEX_FULL should be clear"
        );
    }

    #[test]
    fn test_force_link_10_full() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        phy.force_link(&mut mdio, LinkStatus::slow_full()).unwrap();

        let bmcr_val = mdio.get_register(0, phy_reg::BMCR).unwrap();
        assert_eq!(bmcr_val & bmcr::SPEED_100, 0, "SPEED_100 should be clear");
        assert!(
            bmcr_val & bmcr::DUPLEX_FULL != 0,
            "DUPLEX_FULL should be set"
        );
    }

    // =========================================================================
    // Speed Indication State Machine Tests
    // =========================================================================

    #[test]
    fn test_read_speed_indication_when_an_not_done() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        // AUTODONE bit not set
        mdio.set_register(0, reg::PSCSR, 0x0000);

        let phy = Lan8720a::new(0);
        assert!(phy.read_speed_indication(&mut mdio).unwrap().is_none());
    }

    #[test]
    fn test_read_speed_indication_100fd() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_100FD);

        let phy = Lan8720a::new(0);
        let status = phy.read_speed_indication(&mut mdio).unwrap().unwrap();
        assert_eq!(status.speed, Speed::Mbps100);
        assert_eq!(status.duplex, Duplex::Full);
    }

    #[test]
    fn test_read_speed_indication_100hd() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_100HD);

        let phy = Lan8720a::new(0);
        let status = phy.read_speed_indication(&mut mdio).unwrap().unwrap();
        assert_eq!(status.speed, Speed::Mbps100);
        assert_eq!(status.duplex, Duplex::Half);
    }

    #[test]
    fn test_read_speed_indication_10fd() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_10FD);

        let phy = Lan8720a::new(0);
        let status = phy.read_speed_indication(&mut mdio).unwrap().unwrap();
        assert_eq!(status.speed, Speed::Mbps10);
        assert_eq!(status.duplex, Duplex::Full);
    }

    #[test]
    fn test_read_speed_indication_10hd() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::PSCSR, pscsr::AUTODONE | pscsr::HCDSPEED_10HD);

        let phy = Lan8720a::new(0);
        let status = phy.read_speed_indication(&mut mdio).unwrap().unwrap();
        assert_eq!(status.speed, Speed::Mbps10);
        assert_eq!(status.duplex, Duplex::Half);
    }

    // =========================================================================
    // PHY ID and Capabilities Tests
    // =========================================================================

    #[test]
    fn test_verify_id_returns_true_for_lan8720a() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let phy = Lan8720a::new(0);
        assert!(phy.verify_id(&mut mdio).unwrap());
    }

    #[test]
    fn test_verify_id_returns_false_for_other_phy() {
        let mut mdio = MockMdioBus::new();
        // Set up a different PHY ID (IP101)
        mdio.set_register(0, phy_reg::PHYIDR1, 0x0022);
        mdio.set_register(0, phy_reg::PHYIDR2, 0x1555);

        let phy = Lan8720a::new(0);
        assert!(!phy.verify_id(&mut mdio).unwrap());
    }

    #[test]
    fn test_phy_id_reads_both_registers() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let phy = Lan8720a::new(0);
        let id = phy.phy_id(&mut mdio).unwrap();

        // Should be 0x0007_C0F1 (from setup_lan8720a)
        assert_eq!(id >> 16, 0x0007);
        assert_eq!(id & 0xFFFF, 0xC0F1);
    }

    #[test]
    fn test_revision_extracts_low_bits() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let phy = Lan8720a::new(0);
        let rev = phy.revision(&mut mdio).unwrap();

        // PHYIDR2 = 0xC0F1, revision = 0x1
        assert_eq!(rev, 1);
    }

    #[test]
    fn test_capabilities_reads_from_bmsr() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let phy = Lan8720a::new(0);
        let caps = phy.capabilities(&mut mdio).unwrap();

        assert!(caps.speed_100_fd);
        assert!(caps.speed_100_hd);
        assert!(caps.speed_10_fd);
        assert!(caps.speed_10_hd);
        assert!(caps.auto_negotiation);
    }

    #[test]
    fn test_link_partner_abilities_after_an() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.simulate_link_up_100_fd(0);

        let phy = Lan8720a::new(0);
        let partner = phy.link_partner_abilities(&mut mdio).unwrap();

        assert!(partner.speed_100_fd);
        assert!(partner.speed_100_hd);
        assert!(partner.speed_10_fd);
        assert!(partner.speed_10_hd);
    }

    // =========================================================================
    // Vendor-Specific Feature Tests
    // =========================================================================

    #[test]
    fn test_energy_detect_powerdown_enable() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::MCSR, 0x0000);

        let mut phy = Lan8720a::new(0);
        phy.set_energy_detect_powerdown(&mut mdio, true).unwrap();

        let mcsr = mdio.get_register(0, reg::MCSR).unwrap();
        assert!(mcsr & mcsr::EDPWRDOWN != 0);
    }

    #[test]
    fn test_energy_detect_powerdown_disable() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::MCSR, mcsr::EDPWRDOWN);

        let mut phy = Lan8720a::new(0);
        phy.set_energy_detect_powerdown(&mut mdio, false).unwrap();

        let mcsr = mdio.get_register(0, reg::MCSR).unwrap();
        assert_eq!(mcsr & mcsr::EDPWRDOWN, 0);
    }

    #[test]
    fn test_is_energy_on() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        // No energy
        mdio.set_register(0, reg::MCSR, 0x0000);
        let phy = Lan8720a::new(0);
        assert!(!phy.is_energy_on(&mut mdio).unwrap());

        // Energy detected
        mdio.set_register(0, reg::MCSR, mcsr::ENERGYON);
        assert!(phy.is_energy_on(&mut mdio).unwrap());
    }

    #[test]
    fn test_interrupt_mask() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::IMR, 0x0000);

        let mut phy = Lan8720a::new(0);
        phy.set_interrupt_mask(&mut mdio, isr::LINK_DOWN | isr::AN_COMPLETE)
            .unwrap();

        let imr = mdio.get_register(0, reg::IMR).unwrap();
        assert!(imr & isr::LINK_DOWN != 0);
        assert!(imr & isr::AN_COMPLETE != 0);
    }

    #[test]
    fn test_enable_link_interrupt() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::IMR, 0x0000);

        let mut phy = Lan8720a::new(0);
        phy.enable_link_interrupt(&mut mdio).unwrap();

        let imr = mdio.get_register(0, reg::IMR).unwrap();
        assert!(imr & isr::LINK_DOWN != 0);
        assert!(imr & isr::AN_COMPLETE != 0);
    }

    #[test]
    fn test_symbol_error_count() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);
        mdio.set_register(0, reg::SECR, 42);

        let phy = Lan8720a::new(0);
        assert_eq!(phy.symbol_error_count(&mut mdio).unwrap(), 42);
    }

    #[test]
    fn test_configure_advertisement() {
        let mut mdio = MockMdioBus::new();
        mdio.setup_lan8720a(0);

        let mut phy = Lan8720a::new(0);
        let caps = PhyCapabilities {
            speed_100_fd: true,
            speed_100_hd: false,
            speed_10_fd: true,
            speed_10_hd: false,
            auto_negotiation: true,
            pause: true,
            pause_asymmetric: false,
        };
        phy.configure_advertisement(&mut mdio, &caps).unwrap();

        let anar = mdio.get_register(0, phy_reg::ANAR).unwrap();
        use crate::hal::mdio::anar;
        assert!(anar & anar::TX_FD != 0, "Should advertise 100FD");
        assert_eq!(anar & anar::TX_HD, 0, "Should not advertise 100HD");
        assert!(anar & anar::T10_FD != 0, "Should advertise 10FD");
        assert_eq!(anar & anar::T10_HD, 0, "Should not advertise 10HD");
        assert!(anar & anar::PAUSE != 0, "Should advertise PAUSE");
    }

    // =========================================================================
    // PHY Address Tests
    // =========================================================================

    #[test]
    fn test_phy_address() {
        let phy0 = Lan8720a::new(0);
        assert_eq!(phy0.address(), 0);

        let phy1 = Lan8720a::new(1);
        assert_eq!(phy1.address(), 1);

        let phy31 = Lan8720a::new(31);
        assert_eq!(phy31.address(), 31);
    }

    #[test]
    fn test_operations_use_correct_address() {
        let mut mdio = MockMdioBus::new();
        // Setup PHY at address 5
        mdio.set_register(5, phy_reg::PHYIDR1, 0x0007);
        mdio.set_register(5, phy_reg::PHYIDR2, 0xC0F1);
        mdio.set_register(5, phy_reg::BMSR, 0x7809); // Basic capabilities
        mdio.set_register(5, phy_reg::BMCR, 0x0000);

        let phy = Lan8720a::new(5);
        assert!(phy.verify_id(&mut mdio).unwrap());

        // PHY at address 0 should not verify
        let phy0 = Lan8720a::new(0);
        assert!(!phy0.verify_id(&mut mdio).unwrap());
    }
}
