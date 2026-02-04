//! Internal Implementation Details
//!
//! This module contains implementation details that are not part of the public API.
//! Types in this module may change without notice between minor versions.
//!
//! # Contents
//!
//! - [`register`]: Raw memory-mapped register definitions
//! - [`constants`]: Internal constants and magic numbers
//! - [`phy_registers`]: IEEE 802.3 PHY register definitions
//! - [`gpio_pins`]: GPIO pin assignments for EMAC
//! - [`descriptor_bits`]: DMA descriptor bit field constants
//! - [`lan8720a_regs`]: LAN8720A vendor-specific register definitions
//!
//! # Stability
//!
//! **WARNING:** This module is `pub(crate)` only. Do not depend on any types
//! or functions in this module from external code. They are subject to change
//! without notice.

pub(crate) mod constants;
pub(crate) mod descriptor_bits;
pub(crate) mod gpio_pins;
pub(crate) mod lan8720a_regs;
pub(crate) mod phy_registers;
pub(crate) mod register;

// Register types are accessed via submodules: register::dma::DmaRegs, etc.
