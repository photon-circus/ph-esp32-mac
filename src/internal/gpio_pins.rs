//! ESP32 GPIO Pin Assignments for EMAC
//!
//! This module documents and provides constants for the GPIO pins used by
//! the EMAC peripheral on various ESP32 variants.
//!
//! # Important: Internal Routing
//!
//! The ESP32 EMAC uses **dedicated internal routing** for its RMII data interface.
//! These pins are fixed and cannot be reassigned via the GPIO matrix.
//!
//! # ESP32 RMII Pin Assignments
//!
//! | Signal   | GPIO | Direction | Notes |
//! |----------|------|-----------|-------|
//! | TXD0     | 19   | Output    | Fixed internal routing |
//! | TXD1     | 22   | Output    | Fixed internal routing |
//! | TX_EN    | 21   | Output    | Fixed internal routing |
//! | RXD0     | 25   | Input     | Fixed internal routing |
//! | RXD1     | 26   | Input     | Fixed internal routing |
//! | CRS_DV   | 27   | Input     | Fixed internal routing |
//!
//! # Reference Clock Options
//!
//! | Mode | GPIO | Description |
//! |------|------|-------------|
//! | External input | 0 | 50 MHz from PHY |
//! | Internal output | 16 | 50 MHz to PHY (APLL) |
//! | Internal output | 17 | 50 MHz to PHY (APLL) |
//!
//! # SMI/MDIO Interface
//!
//! The SMI interface pins can be reassigned via GPIO matrix:
//! - **MDC** (clock): Default GPIO23
//! - **MDIO** (data): Default GPIO18

// =============================================================================
// ESP32 GPIO Assignments
// =============================================================================

/// EMAC RMII GPIO assignments for ESP32
#[allow(dead_code)]
#[cfg(feature = "esp32")]
pub mod esp32 {
    // -------------------------------------------------------------------------
    // RMII Data Pins (Fixed Internal Routing)
    // -------------------------------------------------------------------------

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

    // -------------------------------------------------------------------------
    // Reference Clock Pins
    // -------------------------------------------------------------------------

    /// EMAC REF_CLK external input - GPIO0
    pub const REF_CLK_GPIO: u8 = 0;
    /// EMAC REF_CLK output option 1 - GPIO16
    pub const REF_CLK_OUT_GPIO16: u8 = 16;
    /// EMAC REF_CLK output option 2 - GPIO17
    pub const REF_CLK_OUT_GPIO17: u8 = 17;

    // -------------------------------------------------------------------------
    // SMI/MDIO Pins (Configurable via GPIO Matrix)
    // -------------------------------------------------------------------------

    /// Default MDC GPIO (configurable via GPIO matrix)
    pub const MDC_GPIO: u8 = 23;
    /// Default MDIO GPIO (configurable via GPIO matrix)
    pub const MDIO_GPIO: u8 = 18;
}

// =============================================================================
// ESP32-P4 GPIO Assignments (Placeholder)
// =============================================================================

/// EMAC GPIO assignments for ESP32-P4 (experimental placeholder).
#[cfg(feature = "esp32p4")]
#[doc(hidden)]
pub mod esp32p4 {
    // Experimental placeholder: ESP32-P4 has a different EMAC peripheral with
    // different pin mappings. This is intentionally incomplete for this release.

    /// Placeholder for ESP32-P4 TXD0
    pub const TXD0_GPIO: u8 = 0; // TBD
}
