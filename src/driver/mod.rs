//! Core EMAC driver components.
//!
//! This module contains the essential building blocks for configuring and
//! operating the Ethernet MAC controller.
//!
//! # Overview
//!
//! - [`config`]: Configuration types and builder patterns
//! - [`error`]: Error types and result aliases
//! - [`emac`]: The main EMAC controller implementation
//! - [`interrupt`]: Interrupt status handling
//! - [`filtering`]: MAC address, hash, and VLAN filtering
//! - [`flow`]: IEEE 802.3 flow control
//!
//! # Usage
//!
//! ```ignore
//! use ph_esp32_mac::driver::{Emac, EmacConfig};
//!
//! let config = EmacConfig::rmii_esp32_default()
//!     .with_mac_address([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
//! let mut emac: Emac<10, 10, 1600> = Emac::new();
//! emac.init(config, &mut delay)?;
//! ```
//!
//! # See Also
//!
//! - Integration facades (feature-gated modules under `integration`)

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
