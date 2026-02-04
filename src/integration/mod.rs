//! External Stack Integrations
//!
//! This module provides integrations with external libraries and frameworks:
//!
//! - **esp-hal** (`esp_hal`): Integration with the esp-hal hardware abstraction layer
//!   - Interrupt handler registration
//!   - Peripheral ownership patterns
//!   - Requires `esp-hal` feature
//!
//! - **smoltcp** (`smoltcp`): Integration with the smoltcp TCP/IP network stack
//!   - Implements `smoltcp::phy::Device` trait
//!   - RX/TX token support
//!   - Requires `smoltcp` feature
//!
//! - **embassy-net** (`embassy-net`): Integration with Embassy networking
//!   - Implements `embassy_net_driver::Driver` trait
//!   - RX/TX token support and waker-based polling
//!   - Requires `embassy-net` feature
//!
//! # Feature Flags
//!
//! - `esp-hal`: Enables esp-hal integration (`esp_hal` submodule)
//! - `smoltcp`: Enables smoltcp integration (`smoltcp` submodule)
//! - `embassy-net`: Enables Embassy integration (`embassy_net` submodule)
//!
//! # Example
//!
//! ```ignore
//! // With esp-hal
//! use ph_esp32_mac::integration::esp_hal::{EmacExt, Priority};
//! emac.enable_emac_interrupt(handler);
//!
//! // With smoltcp
//! use smoltcp::phy::Device;
//! let (rx, tx) = emac.receive(Instant::ZERO).unwrap();
//!
//! // With embassy-net
//! use embassy_net_driver::Driver;
//! let _ = emac.capabilities();
//! ```

#[cfg(feature = "esp-hal")]
pub mod esp_hal;

#[cfg(feature = "smoltcp")]
pub mod smoltcp;

#[cfg(feature = "embassy-net")]
pub mod embassy_net;

// Re-export key types for convenience when both features are enabled
#[cfg(feature = "esp-hal")]
pub use esp_hal::{EMAC_INTERRUPT, EmacExt, EspHalEmac};

#[cfg(feature = "smoltcp")]
pub use smoltcp::{EmacRxToken, EmacTxToken, ethernet_address};

#[cfg(feature = "embassy-net")]
pub use embassy_net::{EmbassyEmac, EmbassyEmacState, EmbassyRxToken, EmbassyTxToken};
