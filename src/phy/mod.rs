//! Ethernet PHY Drivers
//!
//! This module provides a generic PHY driver trait and implementations for
//! specific PHY chips commonly used with ESP32 Ethernet.
//!
//! # Architecture
//!
//! The PHY layer is designed to be independent of the MAC implementation,
//! communicating only through the MDIO bus interface. This allows:
//!
//! - Reuse across different MAC implementations
//! - Easy addition of new PHY drivers
//! - Testing with mock MDIO implementations
//!
//! # Supported PHY Chips
//!
//! - [`Lan8720a`]: Microchip/SMSC LAN8720A (most common for ESP32)
//!
//! # Example
//!
//! ```ignore
//! use esp32_emac::phy::{Lan8720a, PhyDriver};
//! use esp32_emac::hal::MdioController;
//! use embedded_hal::delay::DelayNs;
//!
//! // Your delay implementation (from esp-hal or custom)
//! let mut delay = /* your DelayNs implementation */;
//!
//! // Create MDIO controller
//! let mut mdio = MdioController::new(&mut delay);
//!
//! // Create PHY driver at address 0
//! let mut phy = Lan8720a::new(0);
//!
//! // Initialize and enable auto-negotiation
//! phy.init(&mut mdio)?;
//!
//! // Poll for link status
//! loop {
//!     if let Some(link) = phy.poll_link(&mut mdio)? {
//!         println!("Link up: {:?}", link);
//!         break;
//!     }
//! }
//! ```
//!
//! # esp-hal Integration Notes
//!
//! For future esp-hal integration, the PHY driver should accept GPIO types
//! for the reset pin:
//!
//! ```ignore
//! use esp32_emac::phy::{Lan8720aWithReset, PhyDriver};
//! use embedded_hal::digital::OutputPin;
//!
//! // With esp-hal GPIO
//! let reset_pin = io.pins.gpio5.into_push_pull_output();
//! let mut phy = Lan8720aWithReset::new(0, reset_pin);
//! phy.hardware_reset(&mut delay)?;
//! phy.init(&mut mdio)?;
//! ```

pub mod generic;
pub mod lan8720a;

pub use generic::{LinkStatus, PhyDriver, PhyCapabilities};
pub use lan8720a::{Lan8720a, Lan8720aWithReset};

// Re-export IEEE 802.3 standard register definitions from mdio
pub use crate::hal::mdio::{phy_reg, bmcr, bmsr, anar};
