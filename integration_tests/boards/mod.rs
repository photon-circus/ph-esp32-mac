//! Board-specific configurations for ESP32 Ethernet development boards
//!
//! This module provides ready-to-use configurations for popular ESP32
//! development boards with built-in Ethernet support.

pub mod wt32_eth01;

// Re-export for convenience
pub use wt32_eth01::Wt32Eth01Config;
