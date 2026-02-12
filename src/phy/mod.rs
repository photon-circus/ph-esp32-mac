//! Ethernet PHY drivers.
//!
//! This module provides a generic PHY driver trait and implementations for
//! common PHY chips used with ESP32 Ethernet.
//!
//! # Overview
//!
//! The PHY layer is independent of the MAC implementation and communicates
//! through the MDIO bus interface. This enables:
//!
//! - Reuse across different MAC implementations
//! - Easy addition of new PHY drivers
//! - Testing with mock MDIO implementations
//!
//! # Supported PHY Chips
//!
//! - [`Lan8720a`]: Microchip/SMSC LAN8720A (most common for ESP32)
//!
//! # Usage
//!
//! ```ignore
//! use ph_esp32_mac::hal::MdioController;
//! use ph_esp32_mac::phy::{Lan8720a, PhyDriver};
//!
//! let mut mdio = MdioController::new(&mut delay);
//! let mut phy = Lan8720a::new(0);
//! phy.init(&mut mdio)?;
//!
//! if let Some(link) = phy.poll_link(&mut mdio)? {
//!     // update MAC config
//! }
//! ```
//!
//! # Reset Pin Support
//!
//! `Lan8720aWithReset` accepts any `embedded_hal::digital::OutputPin`:
//!
//! ```ignore
//! use ph_esp32_mac::phy::{Lan8720aWithReset, PhyDriver};
//!
//! let reset_pin = io.pins.gpio5.into_push_pull_output();
//! let mut phy = Lan8720aWithReset::new(0, reset_pin);
//! phy.hardware_reset(&mut delay)?;
//! phy.init(&mut mdio)?;
//! ```
//!
//! # See Also
//!
//! - [`crate::hal::mdio`] - MDIO bus abstraction

pub mod generic;
pub mod lan8720a;

pub use generic::{LinkStatus, PhyCapabilities, PhyDriver};
pub use lan8720a::{Lan8720a, Lan8720aWithReset};

// Re-export IEEE 802.3 standard register definitions from internal module
// These are implementation details for PHY drivers
#[doc(hidden)]
pub use crate::internal::phy_regs::standard::{anar, anlpar, bmcr, bmsr, phy_reg};
