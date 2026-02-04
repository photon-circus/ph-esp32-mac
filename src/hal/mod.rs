//! Hardware Abstraction Layer
//!
//! This module provides higher-level abstractions over the raw registers,
//! making it easier to use the EMAC peripheral without dealing with
//! register-level details.
//!
//! # Modules
//!
//! - [`clock`]: Clock configuration and control
//! - [`gpio`]: GPIO pin configuration traits and types
//! - [`mdio`]: MDIO/SMI bus for PHY communication
//! - [`reset`]: Reset controller for the EMAC peripheral
//!
//! # Delay Integration
//!
//! All types that require delays use `embedded_hal::delay::DelayNs` directly.
//! Pass any delay implementation from your HAL (e.g., `esp_hal::delay::Delay`).

pub mod clock;
pub mod gpio;
pub mod mdio;
pub mod reset;

// Re-export commonly used types
pub use clock::{ClockController, ClockState};
pub use mdio::{MdcClockDivider, MdioBus, MdioController, PhyStatus};
pub use reset::{ResetController, ResetManager, ResetState};
