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
///
/// **Deprecated:** This module is re-exported for backward compatibility.
/// The canonical location is now `crate::internal::gpio_pins::esp32`.
#[cfg(feature = "esp32")]
#[deprecated(
    since = "0.2.0",
    note = "moved to internal module; will be removed in 0.3.0"
)]
pub mod esp32_gpio {
    pub use crate::internal::gpio_pins::esp32::*;
}
