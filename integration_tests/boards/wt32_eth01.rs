//! WT32-ETH01 Board Configuration
//!
//! The WT32-ETH01 is a compact ESP32 board with integrated Ethernet made by
//! Wireless-Tag. It's one of the most affordable ESP32+Ethernet solutions.
//!
//! # Features
//!
//! - ESP32-WROOM-32E compatible module (WT32-S1)
//! - LAN8720A Ethernet PHY with RMII interface
//! - External 50 MHz crystal oscillator
//! - RJ45 jack with integrated magnetics and LEDs
//! - 4MB flash, no PSRAM
//!
//! # Pin Mapping
//!
//! ## RMII Ethernet Pins (Fixed by ESP32 hardware)
//!
//! | Function | GPIO | Direction | Notes |
//! |----------|------|-----------|-------|
//! | TXD0     | 19   | Output    | TX Data bit 0 |
//! | TXD1     | 22   | Output    | TX Data bit 1 |
//! | TX_EN    | 21   | Output    | TX Enable |
//! | RXD0     | 25   | Input     | RX Data bit 0 |
//! | RXD1     | 26   | Input     | RX Data bit 1 |
//! | CRS_DV   | 27   | Input     | Carrier Sense / Data Valid |
//! | REF_CLK  | 0    | Input     | 50 MHz reference clock |
//!
//! ## SMI (Station Management Interface) Pins
//!
//! | Function | GPIO | Direction | Notes |
//! |----------|------|-----------|-------|
//! | MDC      | 23   | Output    | Management clock |
//! | MDIO     | 18   | Bidir     | Management data |
//!
//! ## Control Pins
//!
//! | Function | GPIO | Notes |
//! |----------|------|-------|
//! | CLK_EN   | 16   | HIGH enables external oscillator |
//! | PHY_RST  | -    | Not connected to GPIO (power-on reset only) |
//!
//! # PHY Configuration
//!
//! The LAN8720A on the WT32-ETH01 has:
//! - PHY Address: **1** (PHYAD0 pulled HIGH)
//! - Clock: External 50 MHz oscillator input on GPIO0
//! - Reset: Power-on reset only (nRST tied to power rail via RC)
//!
//! # Power Notes
//!
//! - Can be powered via 5V pin (has onboard 3.3V regulator)
//! - Do NOT exceed 6V input (regulator may be out of spec)
//! - Do NOT power 3.3V and 5V simultaneously
//!
//! # Programming Notes
//!
//! Requires external USB-TTL adapter:
//! - Pull GPIO0 LOW during reset to enter bootloader
//! - TX/RX are on GPIO1/GPIO3 (directly usable)
//! - Use ESP-IDF/espflash for flashing

/// WT32-ETH01 board configuration constants
pub struct Wt32Eth01Config;

impl Wt32Eth01Config {
    // =========================================================================
    // PHY Configuration
    // =========================================================================
    
    /// PHY address (PHYAD0 is pulled HIGH on WT32-ETH01)
    pub const PHY_ADDR: u8 = 1;
    
    /// PHY type identifier string
    pub const PHY_TYPE: &'static str = "LAN8720A";
    
    /// Expected PHY ID (LAN8720A = 0x0007C0Fx)
    pub const PHY_ID: u32 = 0x0007_C0F0;
    
    /// PHY ID mask (ignores revision nibble)
    pub const PHY_ID_MASK: u32 = 0xFFFF_FFF0;
    
    // =========================================================================
    // SMI (MDIO) Pins
    // =========================================================================
    
    /// MDC (Management Data Clock) GPIO
    pub const MDC_GPIO: u8 = 23;
    
    /// MDIO (Management Data I/O) GPIO
    pub const MDIO_GPIO: u8 = 18;
    
    // =========================================================================
    // Clock Configuration
    // =========================================================================
    
    /// Reference clock input GPIO (50 MHz from external oscillator)
    pub const REF_CLK_GPIO: u8 = 0;
    
    /// Clock enable GPIO (controls external oscillator power)
    /// Set HIGH to enable the oscillator, LOW to disable
    pub const CLK_EN_GPIO: u8 = 16;
    
    /// Reference clock frequency in Hz
    pub const REF_CLK_HZ: u32 = 50_000_000;
    
    // =========================================================================
    // RMII Data Pins (Fixed by ESP32 hardware - for reference only)
    // =========================================================================
    
    /// TX Data 0 GPIO
    pub const TXD0_GPIO: u8 = 19;
    
    /// TX Data 1 GPIO
    pub const TXD1_GPIO: u8 = 22;
    
    /// TX Enable GPIO
    pub const TX_EN_GPIO: u8 = 21;
    
    /// RX Data 0 GPIO
    pub const RXD0_GPIO: u8 = 25;
    
    /// RX Data 1 GPIO
    pub const RXD1_GPIO: u8 = 26;
    
    /// Carrier Sense / Data Valid GPIO
    pub const CRS_DV_GPIO: u8 = 27;
    
    // =========================================================================
    // Reset Configuration
    // =========================================================================
    
    /// PHY reset GPIO (None = not connected, use soft reset)
    pub const PHY_RST_GPIO: Option<u8> = None;
    
    /// Time to wait after enabling oscillator (milliseconds)
    pub const OSC_STARTUP_MS: u32 = 10;
    
    /// Time to wait after PHY reset (milliseconds)  
    pub const PHY_RESET_MS: u32 = 50;
    
    // =========================================================================
    // Board Identification
    // =========================================================================
    
    /// Board name
    pub const BOARD_NAME: &'static str = "WT32-ETH01";
    
    /// Board manufacturer
    pub const MANUFACTURER: &'static str = "Wireless-Tag";
    
    /// ESP32 module on board
    pub const MODULE: &'static str = "WT32-S1";
    
    // =========================================================================
    // Helper Methods
    // =========================================================================
    
    /// Check if a PHY ID matches the expected LAN8720A
    #[inline]
    pub const fn is_valid_phy_id(id: u32) -> bool {
        (id & Self::PHY_ID_MASK) == Self::PHY_ID
    }
    
    /// Get a human-readable description of the board
    pub const fn description() -> &'static str {
        "WT32-ETH01: ESP32 + LAN8720A Ethernet (RMII, 50MHz external clock, PHY addr 1)"
    }
}

// =============================================================================
// EmacConfig Builder Extension
// =============================================================================

use ph_esp32_mac::{EmacConfig, PhyInterface, RmiiClockMode};

/// Extension trait to easily configure EMAC for WT32-ETH01
pub trait Wt32Eth01Ext {
    /// Configure EMAC for WT32-ETH01 board
    ///
    /// Sets:
    /// - RMII interface
    /// - External clock input on GPIO0
    /// - MDC on GPIO23, MDIO on GPIO18
    fn for_wt32_eth01(self) -> Self;
    
    /// Configure EMAC for WT32-ETH01 with custom MAC address
    fn for_wt32_eth01_with_mac(self, mac: [u8; 6]) -> Self;
}

impl Wt32Eth01Ext for EmacConfig {
    fn for_wt32_eth01(self) -> Self {
        // Note: MDC (GPIO23) and MDIO (GPIO18) are fixed by ESP32 EMAC hardware
        self.with_phy_interface(PhyInterface::Rmii)
            .with_rmii_clock(RmiiClockMode::ExternalInput {
                gpio: Wt32Eth01Config::REF_CLK_GPIO,
            })
    }
    
    fn for_wt32_eth01_with_mac(self, mac: [u8; 6]) -> Self {
        self.with_mac_address(mac).for_wt32_eth01()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_phy_id_validation() {
        // LAN8720A with various revisions
        assert!(Wt32Eth01Config::is_valid_phy_id(0x0007_C0F0));
        assert!(Wt32Eth01Config::is_valid_phy_id(0x0007_C0F1));
        assert!(Wt32Eth01Config::is_valid_phy_id(0x0007_C0FF));
        
        // Other PHYs should not match
        assert!(!Wt32Eth01Config::is_valid_phy_id(0x0022_1556)); // KSZ8081
        assert!(!Wt32Eth01Config::is_valid_phy_id(0x0000_0000));
    }
    
    #[test]
    fn test_pin_assignments() {
        // Verify critical pin assignments match WT32-ETH01 hardware
        assert_eq!(Wt32Eth01Config::PHY_ADDR, 1);
        assert_eq!(Wt32Eth01Config::CLK_EN_GPIO, 16);
        assert_eq!(Wt32Eth01Config::REF_CLK_GPIO, 0);
        assert_eq!(Wt32Eth01Config::MDC_GPIO, 23);
        assert_eq!(Wt32Eth01Config::MDIO_GPIO, 18);
    }
}
