//! Board-specific helpers and pin mappings.
//!
//! This module provides opinionated board configurations to reduce boilerplate
//! for common ESP32 Ethernet boards.
//!
//! # Overview
//!
//! The board helpers encapsulate MAC/PHY defaults and wiring assumptions for a
//! specific board. They are intended to define a canonical "happy path" for
//! esp-hal users.
//!
//! # Supported Boards
//!
//! - WT32-ETH01 (LAN8720A, external 50 MHz oscillator)
//!
//! # See Also
//!
//! - [`crate::integration::esp_hal`] - esp-hal facade helpers

#[cfg(feature = "esp32")]
#[cfg_attr(docsrs, doc(cfg(feature = "esp32")))]
pub mod wt32_eth01;
