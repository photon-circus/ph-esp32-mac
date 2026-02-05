//! Board-specific helpers and pin mappings.
//!
//! This module provides opinionated board configurations to reduce boilerplate
//! for common ESP32 Ethernet boards.

#[cfg(feature = "esp32")]
#[cfg_attr(docsrs, doc(cfg(feature = "esp32")))]
pub mod wt32_eth01;
