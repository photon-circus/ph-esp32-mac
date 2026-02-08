//! Hardware abstraction layer.
//!
//! This module provides higher-level abstractions over the raw registers for
//! clock/reset control and MDIO access.
//!
//! # Overview
//!
//! - [`clock`]: Clock configuration and control
//! - [`mdio`]: MDIO/SMI bus for PHY communication
//! - [`reset`]: Reset controller for the EMAC peripheral
//!
//! # Usage
//!
//! ```ignore
//! use ph_esp32_mac::hal::MdioController;
//!
//! let mut mdio = MdioController::new(&mut delay);
//! let phy_id = mdio.read(0, 2)?;
//! ```
//!
//! # Delay Integration
//!
//! All types that require delays use `embedded_hal::delay::DelayNs` directly.
//! Pass any delay implementation from your HAL (e.g., `esp_hal::delay::Delay`).
//!
//! # See Also
//!
//! - [`crate::phy`] - PHY drivers that consume the MDIO bus

pub mod clock;
pub mod mdio;
pub mod reset;

// Re-export commonly used types
pub use clock::{ClockController, ClockState};
pub use mdio::{MdcClockDivider, MdioBus, MdioController, PhyStatus};
pub use reset::{ResetController, ResetManager, ResetState};
