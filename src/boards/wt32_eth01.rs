//! WT32-ETH01 board configuration (ESP32 + LAN8720A).
//!
//! This module provides constants and helpers for the WT32-ETH01 board to
//! reduce boilerplate in esp-hal bring-up code. It is intended as the
//! canonical "happy path" for esp-hal async examples.

use crate::{EmacConfig, Lan8720a, PhyInterface, RmiiClockMode};

/// WT32-ETH01 board configuration constants and helpers.
pub struct Wt32Eth01;

impl Wt32Eth01 {
    // =========================================================================
    // PHY Configuration
    // =========================================================================

    /// PHY address (PHYAD0 is pulled HIGH on WT32-ETH01).
    pub const PHY_ADDR: u8 = 1;

    /// Expected PHY ID (LAN8720A = 0x0007C0Fx).
    pub const PHY_ID: u32 = 0x0007_C0F0;

    /// PHY ID mask (ignores revision nibble).
    pub const PHY_ID_MASK: u32 = 0xFFFF_FFF0;

    // =========================================================================
    // SMI (MDIO) Pins
    // =========================================================================

    /// MDC (Management Data Clock) GPIO.
    pub const MDC_GPIO: u8 = 23;

    /// MDIO (Management Data I/O) GPIO.
    pub const MDIO_GPIO: u8 = 18;

    // =========================================================================
    // Clock Configuration
    // =========================================================================

    /// Reference clock input GPIO (50 MHz from external oscillator).
    pub const REF_CLK_GPIO: u8 = 0;

    /// Clock enable GPIO (controls external oscillator power).
    /// Set HIGH to enable the oscillator, LOW to disable.
    pub const CLK_EN_GPIO: u8 = 16;

    /// Reference clock frequency in Hz.
    pub const REF_CLK_HZ: u32 = 50_000_000;

    // =========================================================================
    // RMII Data Pins (Fixed by ESP32 hardware - for reference only)
    // =========================================================================

    /// TX Data 0 GPIO.
    pub const TXD0_GPIO: u8 = 19;

    /// TX Data 1 GPIO.
    pub const TXD1_GPIO: u8 = 22;

    /// TX Enable GPIO.
    pub const TX_EN_GPIO: u8 = 21;

    /// RX Data 0 GPIO.
    pub const RXD0_GPIO: u8 = 25;

    /// RX Data 1 GPIO.
    pub const RXD1_GPIO: u8 = 26;

    /// Carrier Sense / Data Valid GPIO.
    pub const CRS_DV_GPIO: u8 = 27;

    // =========================================================================
    // Reset Configuration
    // =========================================================================

    /// PHY reset GPIO (None = not connected, use soft reset).
    pub const PHY_RST_GPIO: Option<u8> = None;

    /// Time to wait after enabling oscillator (milliseconds).
    pub const OSC_STARTUP_MS: u32 = 10;

    /// Time to wait after PHY reset (milliseconds).
    pub const PHY_RESET_MS: u32 = 50;

    // =========================================================================
    // Board Identification
    // =========================================================================

    /// Board name.
    pub const BOARD_NAME: &'static str = "WT32-ETH01";

    /// Board manufacturer.
    pub const MANUFACTURER: &'static str = "Wireless-Tag";

    /// ESP32 module on board.
    pub const MODULE: &'static str = "WT32-S1";

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Check if a PHY ID matches the expected LAN8720A pattern.
    #[inline]
    pub const fn is_valid_phy_id(id: u32) -> bool {
        (id & Self::PHY_ID_MASK) == Self::PHY_ID
    }

    /// Return the default EMAC configuration for WT32-ETH01.
    ///
    /// # Returns
    ///
    /// A configuration using RMII with external reference clock on GPIO0.
    #[must_use]
    pub const fn emac_config() -> EmacConfig {
        EmacConfig::rmii_esp32_default()
            .with_phy_interface(PhyInterface::Rmii)
            .with_rmii_clock(RmiiClockMode::ExternalInput {
                gpio: Self::REF_CLK_GPIO,
            })
    }

    /// Return the default EMAC configuration with a custom MAC address.
    ///
    /// # Arguments
    ///
    /// * `mac` - 6-byte MAC address.
    ///
    /// # Returns
    ///
    /// A configuration using RMII with external reference clock on GPIO0.
    #[must_use]
    pub const fn emac_config_with_mac(mac: [u8; 6]) -> EmacConfig {
        Self::emac_config().with_mac_address(mac)
    }

    /// Construct a LAN8720A PHY driver using the board's PHY address.
    #[must_use]
    pub const fn lan8720a() -> Lan8720a {
        Lan8720a::new(Self::PHY_ADDR)
    }

    /// Get a human-readable description of the board.
    #[must_use]
    pub const fn description() -> &'static str {
        "WT32-ETH01: ESP32 + LAN8720A Ethernet (RMII, 50MHz external clock, PHY addr 1)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phy_id_validation() {
        assert!(Wt32Eth01::is_valid_phy_id(0x0007_C0F0));
        assert!(Wt32Eth01::is_valid_phy_id(0x0007_C0F1));
        assert!(Wt32Eth01::is_valid_phy_id(0x0007_C0FF));
        assert!(!Wt32Eth01::is_valid_phy_id(0x0022_1556));
    }

    #[test]
    fn pin_assignments_match_board() {
        assert_eq!(Wt32Eth01::PHY_ADDR, 1);
        assert_eq!(Wt32Eth01::CLK_EN_GPIO, 16);
        assert_eq!(Wt32Eth01::REF_CLK_GPIO, 0);
        assert_eq!(Wt32Eth01::MDC_GPIO, 23);
        assert_eq!(Wt32Eth01::MDIO_GPIO, 18);
    }
}
