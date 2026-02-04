//! PHY Register Definitions
//!
//! This module contains register definitions for PHY devices accessed via MDIO.
//! These are distinct from the ESP32 memory-mapped peripheral registers in
//! [`register`](super::register).
//!
//! # Module Organization
//!
//! - [`standard`] - IEEE 802.3 Clause 22 standard PHY registers (0-15)
//! - [`lan8720a`] - LAN8720A vendor-specific registers (16-31)
//!
//! # Access Method
//!
//! PHY registers are accessed via the MDIO (Management Data Input/Output)
//! interface, not direct memory mapping. The EMAC's MDIO controller
//! handles the serial protocol.

pub mod lan8720a;
pub mod standard;
