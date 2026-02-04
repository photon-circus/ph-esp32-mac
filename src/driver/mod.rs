//! Core driver components for the ESP32 EMAC peripheral.
//!
//! This module contains the essential building blocks for configuring and
//! operating the Ethernet MAC controller:
//!
//! - [`config`] - Configuration types and builder patterns
//! - [`error`] - Error types and result aliases
//! - [`mac`] - The main EMAC controller implementation
//!
//! # Example
//!
//! ```ignore
//! use ph_esp32_mac::driver::{EmacConfig, Emac, Error};
//!
//! let config = EmacConfig::new()
//!     .with_mac_address([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
//! ```

// Submodules
pub mod config;
pub mod error;
pub mod mac;

// Re-exports for convenience
pub use config::{
    ChecksumConfig, DmaBurstLen, Duplex, EmacConfig, FlowControlConfig, MacAddressFilter,
    MacFilterType, PauseLowThreshold, PhyInterface, RmiiClockMode, Speed, State, TxChecksumMode,
    MAC_FILTER_SLOTS,
};
pub use error::{ConfigError, ConfigResult, DmaError, DmaResult, Error, IoError, IoResult, Result};
pub use mac::{Emac, EmacDefault, EmacLarge, EmacSmall, InterruptStatus};
