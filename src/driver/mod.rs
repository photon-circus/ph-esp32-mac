//! Core driver components for the ESP32 EMAC peripheral.
//!
//! This module contains the essential building blocks for configuring and
//! operating the Ethernet MAC controller:
//!
//! - [`config`] - Configuration types and builder patterns
//! - [`error`] - Error types and result aliases
//! - [`emac`] - The main EMAC controller implementation
//! - [`interrupt`] - Interrupt status handling
//! - [`filtering`] - MAC address, hash, and VLAN filtering
//! - [`flow`] - IEEE 802.3 flow control
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
pub mod emac;
pub mod error;
pub mod filtering;
pub mod flow;
pub mod interrupt;

// Re-exports for convenience
pub use config::{
    ChecksumConfig, DmaBurstLen, Duplex, EmacConfig, FlowControlConfig, MAC_FILTER_SLOTS,
    MacAddressFilter, MacFilterType, PauseLowThreshold, PhyInterface, RmiiClockMode, Speed, State,
    TxChecksumMode,
};
pub use emac::{Emac, EmacDefault, EmacLarge, EmacSmall};
pub use error::{ConfigError, ConfigResult, DmaError, DmaResult, Error, IoError, IoResult, Result};
pub use interrupt::InterruptStatus;
