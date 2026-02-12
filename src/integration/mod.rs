//! Integration facades for external stacks and runtimes.
//!
//! This module provides optional adapters for common embedded networking
//! stacks and runtimes.
//!
//! # Overview
//!
//! - **esp-hal** (`esp_hal`): HAL-friendly builders and ISR helpers
//! - **smoltcp** (`smoltcp`): `smoltcp::phy::Device` implementation
//! - **embassy-net** (`embassy-net`): `embassy_net_driver::Driver` implementation
//!
//! # Feature Flags
//!
//! - `esp-hal`: Enables esp-hal integration (`esp_hal` submodule)
//! - `smoltcp`: Enables smoltcp integration (`smoltcp` submodule)
//! - `embassy-net`: Enables Embassy integration (`embassy_net` submodule)
//!
//! # Usage
//!
//! ```ignore
//! // esp-hal
//! use ph_esp32_mac::integration::esp_hal::{EmacExt, Priority};
//! emac.bind_interrupt(handler);
//!
//! // smoltcp
//! use smoltcp::phy::Device;
//! let (rx, tx) = emac.receive(Instant::ZERO).unwrap();
//!
//! // embassy-net
//! use embassy_net_driver::Driver;
//! let _ = emac.capabilities();
//! ```
//!
//! # See Also
//!
//! - [`crate::esp_hal`] - re-exported esp-hal facade at the crate root

#[cfg(feature = "esp-hal")]
#[cfg_attr(docsrs, doc(cfg(feature = "esp-hal")))]
pub mod esp_hal;

#[cfg(feature = "smoltcp")]
#[cfg_attr(docsrs, doc(cfg(feature = "smoltcp")))]
pub mod smoltcp;

#[cfg(feature = "embassy-net")]
#[cfg_attr(docsrs, doc(cfg(feature = "embassy-net")))]
pub mod embassy_net;

// Re-export key types for convenience when both features are enabled
#[cfg(feature = "esp-hal")]
#[cfg_attr(docsrs, doc(cfg(feature = "esp-hal")))]
pub use esp_hal::{EMAC_INTERRUPT, EmacBuilder, EmacExt, EmacPhyBundle};

#[cfg(feature = "smoltcp")]
#[cfg_attr(docsrs, doc(cfg(feature = "smoltcp")))]
pub use smoltcp::{EmacRxToken, EmacTxToken, ethernet_address};

#[cfg(feature = "embassy-net")]
#[cfg_attr(docsrs, doc(cfg(feature = "embassy-net")))]
pub use embassy_net::{EmbassyEmac, EmbassyEmacState, EmbassyRxToken, EmbassyTxToken};
