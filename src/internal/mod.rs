//! Internal Implementation Details
//!
//! This module contains implementation details that are not part of the public API.
//! Types in this module may change without notice between minor versions.
//!
//! # Contents
//!
//! - [`register`]: Raw memory-mapped register definitions
//! - [`phy_regs`]: PHY register definitions (MDIO-accessed)
//! - [`constants`]: Internal constants and magic numbers
//! - [`gpio_pins`]: GPIO pin assignments for EMAC
//! - [`dma`]: DMA engine and descriptor management
//!
//! # Stability
//!
//! **WARNING:** This module is `pub(crate)` only. Do not depend on any types
//! or functions in this module from external code. They are subject to change
//! without notice.

pub(crate) mod constants;
pub(crate) mod dma;
pub(crate) mod gpio_pins;
pub(crate) mod phy_regs;
pub(crate) mod register;

// Register types are accessed via submodules: register::dma::DmaRegs, etc.
