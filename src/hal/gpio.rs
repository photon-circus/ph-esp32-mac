//! GPIO Pin Information
//!
//! This module documents the GPIO pins used by the EMAC peripheral.
//!
//! # Important: Internal Routing
//!
//! The ESP32 EMAC uses **dedicated internal routing** for its RMII data interface.
//! The following pins are fixed and cannot be reassigned via the GPIO matrix:
//!
//! | Signal   | GPIO | Direction |
//! |----------|------|-----------|
//! | TXD0     | 19   | Output    |
//! | TXD1     | 22   | Output    |
//! | TX_EN    | 21   | Output    |
//! | RXD0     | 25   | Input     |
//! | RXD1     | 26   | Input     |
//! | CRS_DV   | 27   | Input     |
//!
//! # Reference Clock
//!
//! The 50 MHz reference clock can be configured as:
//! - **External input**: GPIO0 (default)
//! - **Internal output**: GPIO16 or GPIO17 (requires external clock from PHY)
//!
//! # SMI/MDIO Interface
//!
//! The SMI interface for PHY register access uses the hardware GMACMIIADDR and
//! GMACMIIDATA registers. The physical MDC/MDIO pins are typically:
//! - **MDC** (clock): GPIO23
//! - **MDIO** (data): GPIO18
//!
//! However, these can be routed to other pins via the GPIO matrix if needed.
//! The MDIO protocol is handled in hardware by the MAC, not bit-banged in software.
//!
//! # Usage
//!
//! Since the EMAC uses internal routing, no GPIO configuration is required
//! in this driver. The pins are automatically configured when the EMAC is
//! initialized. Users only need to ensure:
//!
//! 1. The required pins are not used for other functions
//! 2. External PHY is properly connected to the correct pins
//! 3. Reference clock source is configured via `EmacConfig::rmii_clock`

/// EMAC RMII GPIO assignments for ESP32
#[cfg(feature = "esp32")]
pub mod esp32_gpio {
    /// EMAC TXD0 - GPIO19 (fixed, internal routing)
    pub const TXD0_GPIO: u8 = 19;
    /// EMAC TXD1 - GPIO22 (fixed, internal routing)
    pub const TXD1_GPIO: u8 = 22;
    /// EMAC TX_EN - GPIO21 (fixed, internal routing)
    pub const TX_EN_GPIO: u8 = 21;
    /// EMAC RXD0 - GPIO25 (fixed, internal routing)
    pub const RXD0_GPIO: u8 = 25;
    /// EMAC RXD1 - GPIO26 (fixed, internal routing)
    pub const RXD1_GPIO: u8 = 26;
    /// EMAC CRS_DV - GPIO27 (fixed, internal routing)
    pub const CRS_DV_GPIO: u8 = 27;
    /// EMAC REF_CLK external input - GPIO0
    pub const REF_CLK_GPIO: u8 = 0;
    /// EMAC REF_CLK output option 1 - GPIO16
    pub const REF_CLK_OUT_GPIO16: u8 = 16;
    /// EMAC REF_CLK output option 2 - GPIO17
    pub const REF_CLK_OUT_GPIO17: u8 = 17;
    /// Default MDC GPIO (configurable via GPIO matrix)
    pub const MDC_GPIO: u8 = 23;
    /// Default MDIO GPIO (configurable via GPIO matrix)
    pub const MDIO_GPIO: u8 = 18;
}
