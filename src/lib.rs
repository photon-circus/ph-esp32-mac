//! ESP32 EMAC Driver
//!
//! A `no_std`, `no_alloc` Rust implementation of the ESP32 Ethernet MAC (EMAC) controller.
//!
//! This crate provides a bare-metal driver for the ESP32's built-in Ethernet MAC,
//! based on the Synopsys DesignWare MAC (DWMAC) IP core.
//!
//! # Architecture
//!
//! The driver is organized into three layers:
//!
//! 1. **MAC Layer** ([`mac`]): Main EMAC driver with TX/RX operations
//! 2. **PHY Layer** ([`phy`]): Ethernet PHY drivers (e.g., LAN8720A)
//! 3. **HAL Layer** ([`hal`]): Hardware abstraction for clocks, GPIO, MDIO
//!
//! ## Standard Compliance
//!
//! - **IEEE 802.3**: Frame sizes, MDIO/MDC protocol, flow control
//! - **Synopsys DWMAC**: DMA descriptors, register layout (portable to other SoCs)
//! - **ESP32-specific**: Memory map, clock configuration, GPIO routing
//!
//! # Supported PHY Chips
//!
//! - [`Lan8720a`]: Microchip/SMSC LAN8720A (most common, RMII interface)
//!
//! Additional PHY drivers can be added by implementing [`PhyDriver`].
//!
//! This release targets ESP32 only.
//!
//! # Features
//!
//! - `esp32` (default): Target the original ESP32
//! - `defmt`: Enable defmt formatting for error types
//! - `smoltcp`: Enable smoltcp network stack integration
//! - `critical-section`: Enable ISR-safe `SharedEmac` wrapper
//! - `async`: Enable async/await support with wakers
//! - `esp-hal`: Enable esp-hal ergonomic integration
//! - `embassy-net`: Enable embassy-net-driver integration
//!
//! # Example
//!
//! ```ignore
//! use esp32_emac::{Emac, EmacConfig, Lan8720a, PhyDriver};
//! use esp32_emac::hal::MdioController;
//! use embedded_hal::delay::DelayNs;
//!
//! // Your delay implementation (from esp-hal or custom)
//! let mut delay = /* your DelayNs implementation */;
//!
//! // Static allocation
//! static mut EMAC: Emac<10, 10, 1600> = Emac::new();
//!
//! let emac = unsafe { &mut EMAC };
//!
//! // Configure with builder pattern
//! let config = EmacConfig::new()
//!     .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
//!     .with_phy_interface(PhyInterface::Rmii);
//!
//! emac.init(config, &mut delay).unwrap();
//!
//! // Initialize PHY
//! let mut mdio = MdioController::new(&mut delay);
//! let mut phy = Lan8720a::new(0);
//! phy.init(&mut mdio).unwrap();
//!
//! // Wait for link and configure MAC
//! if let Some(link) = phy.poll_link(&mut mdio).unwrap() {
//!     emac.set_speed(link.speed);
//!     emac.set_duplex(link.duplex);
//! }
//!
//! emac.start().unwrap();
//! ```
//!
//! # Memory Requirements
//!
//! With default configuration (10 RX buffers, 10 TX buffers, 1600 bytes each):
//! - Total: ~32 KB of DMA-capable SRAM
//!
//! # esp-hal Integration Path
//!
//! This crate is designed for eventual integration with `esp-hal`. The following
//! changes would be needed:
//!
//! ## Peripheral Ownership
//! ```ignore
//! // Current (standalone)
//! static mut EMAC: Emac<10, 10, 1600> = Emac::new();
//!
//! // Future esp-hal style
//! let emac = Emac::new(peripherals.EMAC, peripherals.DMA, config);
//! ```
//!
//! ## GPIO Pins
//!
//! The ESP32 EMAC uses **dedicated internal routing** for RMII data pins.
//! These pins are fixed and automatically configured - see [`hal::gpio::esp32_gpio`]
//! for pin assignments. No user configuration is needed.
//!
//! ## Async Support
//! ```ignore
//! // Future async
//! impl<'d> Emac<'d, Async> {
//!     pub async fn receive(&mut self) -> Result<Frame> { ... }
//! }
//! ```
//!
//! # Completed Improvements
//!
//! - ✅ Centralized constants module (`constants.rs`)
//! - ✅ Register accessor macros (`reg_rw!`, `reg_ro!`, etc.)
//! - ✅ smoltcp integration with proper safety documentation
//! - ✅ Conditional defmt support
//! - ✅ Builder pattern for `EmacConfig` (`with_*` methods)
//! - ✅ Split error types (`ConfigError`, `DmaError`, `IoError`)
//! - ✅ PHY driver abstraction with LAN8720A support

#![no_std]
#![deny(missing_docs)]
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
// Clippy lint levels live here; thresholds and config are in clippy.toml.
#![deny(clippy::correctness)]
#![warn(
    clippy::suspicious,
    clippy::style,
    clippy::complexity,
    clippy::perf,
    clippy::cloned_instead_of_copied,
    clippy::explicit_iter_loop,
    clippy::implicit_clone,
    clippy::inconsistent_struct_constructor,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned,
    clippy::uninlined_format_args,
    clippy::unnested_or_patterns,
    clippy::std_instead_of_core,
    clippy::std_instead_of_alloc,
    clippy::alloc_instead_of_core
)]
#![allow(
    clippy::mod_module_files,
    clippy::self_named_module_files,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::struct_excessive_bools,
    clippy::fn_params_excessive_bools,
    clippy::type_complexity,
    clippy::must_use_candidate,
    clippy::assertions_on_constants,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::panic_in_result_fn,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::module_name_repetitions,
    clippy::wildcard_imports,
    clippy::items_after_statements,
    clippy::let_underscore_future
)]
#[cfg(all(feature = "esp32", feature = "esp32p4"))]
compile_error!("Features 'esp32' and 'esp32p4' are mutually exclusive.");

#[cfg(not(any(feature = "esp32", feature = "esp32p4")))]
compile_error!("Either feature 'esp32' or 'esp32p4' must be enabled. The default is 'esp32'.");
// #![allow(dead_code)] // Temporarily disabled to identify unused code

// =============================================================================
// Modules
// =============================================================================

pub mod driver;
pub mod hal;
pub mod phy;

// Internal implementation details (pub(crate) only)
mod internal;

#[cfg(any(feature = "smoltcp", feature = "esp-hal", feature = "embassy-net"))]
pub mod integration;

#[cfg(feature = "critical-section")]
pub mod sync;

// Test utilities (only available during testing)
#[cfg(test)]
pub mod testing;

// =============================================================================
// Re-exports
// =============================================================================

pub use driver::config::{
    ChecksumConfig, DmaBurstLen, Duplex, EmacConfig, FlowControlConfig, MAC_FILTER_SLOTS,
    MacAddressFilter, MacFilterType, PauseLowThreshold, PhyInterface, RmiiClockMode, Speed, State,
    TxChecksumMode,
};
pub use driver::emac::{Emac, EmacDefault, EmacLarge, EmacSmall};
pub use driver::error::{
    ConfigError, ConfigResult, DmaError, DmaResult, Error, IoError, IoResult, Result,
};
pub use driver::interrupt::InterruptStatus;

// Re-export register access types
pub use internal::register::dma::DmaRegs;
pub use internal::register::ext::ExtRegs;
pub use internal::register::mac::MacRegs;

// Re-export HAL types
pub use hal::{
    ClockController, ClockState, MdcClockDivider, MdioBus, MdioController, PhyStatus,
    ResetController, ResetManager, ResetState,
};

// Re-export PHY types
pub use phy::{Lan8720a, Lan8720aWithReset, LinkStatus, PhyCapabilities, PhyDriver};

// Re-export sync types when critical-section is enabled
#[cfg(feature = "critical-section")]
pub use sync::{SharedEmac, SharedEmacDefault, SharedEmacLarge, SharedEmacSmall};

// Re-export async types when async feature is enabled
#[cfg(feature = "async")]
pub use sync::asynch::{AsyncEmacExt, RX_WAKER, TX_WAKER, async_interrupt_handler};

// Re-export embassy-net types when embassy-net feature is enabled
#[cfg(feature = "embassy-net")]
pub use integration::embassy_net::{EmbassyEmac, EmbassyEmacState, EmbassyRxToken, EmbassyTxToken};

// =============================================================================
// Constants (re-exported from internal constants module)
// =============================================================================

pub use internal::constants::{
    // Frame/buffer sizes
    CRC_SIZE,
    DEFAULT_BUFFER_SIZE,
    // Flow control
    DEFAULT_FLOW_HIGH_WATER,
    DEFAULT_FLOW_LOW_WATER,
    // MAC address
    DEFAULT_MAC_ADDR,
    // Buffer counts
    DEFAULT_RX_BUFFERS,
    DEFAULT_TX_BUFFERS,
    ETH_HEADER_SIZE,
    // Timing
    FLUSH_TIMEOUT,
    MAC_ADDR_LEN,
    MAX_FRAME_SIZE,
    // Clocks
    MDC_MAX_FREQ_HZ,
    MII_10M_CLK_HZ,
    MII_100M_CLK_HZ,
    MII_BUSY_TIMEOUT,
    MIN_FRAME_SIZE,
    MTU,
    PAUSE_TIME_MAX,
    RESET_POLL_INTERVAL_US,
    RMII_CLK_HZ,
    SOFT_RESET_TIMEOUT_MS,
    VLAN_TAG_SIZE,
};
