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
//! 1. **MAC Layer** ([`driver::emac`]): Main EMAC driver with TX/RX operations
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
//! These pins are fixed and automatically configured - see [`boards::wt32_eth01::Wt32Eth01`]
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

#![cfg_attr(docsrs, doc(cfg_hide(feature = "esp32p4")))]
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

#[cfg(feature = "esp32")]
#[cfg_attr(docsrs, doc(cfg(feature = "esp32")))]
pub mod boards;
pub mod driver;
pub mod hal;
pub mod phy;

// Internal implementation details (pub(crate) only)
mod internal;

#[cfg(any(feature = "smoltcp", feature = "esp-hal", feature = "embassy-net"))]
#[cfg_attr(
    docsrs,
    doc(cfg(any(feature = "smoltcp", feature = "esp-hal", feature = "embassy-net")))
)]
pub mod integration;

#[cfg(feature = "critical-section")]
#[cfg_attr(docsrs, doc(cfg(feature = "critical-section")))]
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

/// Low-level register accessors for advanced use.
///
/// These are intentionally separated from the primary facade. Most users should
/// prefer the safe driver APIs instead of touching registers directly.
///
/// # Safety
///
/// Direct register access bypasses driver invariants. Use only if you fully
/// understand the ESP32 EMAC hardware and accept responsibility for correct
/// sequencing and synchronization.
pub mod unsafe_registers {
    pub use crate::internal::register::dma::DmaRegs;
    pub use crate::internal::register::ext::ExtRegs;
    pub use crate::internal::register::mac::MacRegs;
}

// Re-export PHY types
pub use phy::{Lan8720a, Lan8720aWithReset, LinkStatus, PhyCapabilities, PhyDriver};

// Re-export sync types when critical-section is enabled
#[cfg(feature = "critical-section")]
pub use sync::{SharedEmac, SharedEmacDefault, SharedEmacLarge, SharedEmacSmall};

// esp-hal facade re-export (for ergonomic access)
#[cfg(feature = "esp-hal")]
pub mod esp_hal {
    //! esp-hal integration facade.
    //!
    //! This module re-exports esp-hal integration helpers for ergonomic access.

    #![cfg_attr(docsrs, doc(cfg(feature = "esp-hal")))]

    #[cfg(feature = "esp32")]
    pub use crate::integration::esp_hal::Wt32Eth01;
    pub use crate::integration::esp_hal::{
        Delay, EMAC_INTERRUPT, EmacBuilder, EmacExt, EmacPhyBundle, Interrupt, InterruptHandler,
        Priority,
    };
    pub use crate::{emac_async_isr, emac_isr};
}

// Re-export async types when async feature is enabled
#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
pub use sync::asynch::{AsyncEmacExt, AsyncEmacState, async_interrupt_handler};

// Re-export embassy-net types when embassy-net feature is enabled
#[cfg(feature = "embassy-net")]
#[cfg_attr(docsrs, doc(cfg(feature = "embassy-net")))]
pub use integration::embassy_net::{EmbassyEmac, EmbassyEmacState, EmbassyRxToken, EmbassyTxToken};

/// Shared driver constants.
///
/// These are grouped into a dedicated module to keep the top-level facade
/// focused on driver types and integration points.
pub mod constants {
    pub use crate::internal::constants::{
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
}

// =============================================================================
// Macro Helpers
// =============================================================================

/// Declare a static, ISR-safe EMAC instance for synchronous use.
///
/// This macro expands to a `SharedEmac` static placed in DMA-capable memory on
/// ESP32, reducing boilerplate for esp-hal synchronous bring-up.
///
/// # Examples
///
/// ```ignore
/// ph_esp32_mac::emac_static_sync!(EMAC);
///
/// EMAC.with(|emac| {
///     emac.init(EmacConfig::rmii_esp32_default(), &mut delay).unwrap();
///     emac.start().unwrap();
/// });
/// ```
#[cfg(feature = "critical-section")]
#[macro_export]
macro_rules! emac_static_sync {
    ($name:ident) => {
        $crate::emac_static_sync!($name, 10, 10, 1600);
    };
    ($name:ident, $rx:expr, $tx:expr, $buf:expr) => {
        #[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".dram1"))]
        static $name: $crate::sync::SharedEmac<$rx, $tx, $buf> = $crate::sync::SharedEmac::new();
    };
}

/// Declare static storage for async EMAC usage (EMAC + AsyncEmacState).
///
/// This macro expands to a `StaticCell<Emac<..>>` and an `AsyncEmacState`.
/// It requires the `async` feature and a `static_cell` dependency in your
/// application crate.
///
/// # Examples
///
/// ```ignore
/// ph_esp32_mac::emac_static_async!(EMAC, ASYNC_STATE);
///
/// emac_async_isr!(EMAC_IRQ, Priority::Priority1, &ASYNC_STATE);
/// let emac_ptr = EMAC.init(Emac::new()) as *mut Emac<10, 10, 1600>;
/// ```
#[cfg(feature = "async")]
#[macro_export]
macro_rules! emac_static_async {
    ($emac:ident, $state:ident) => {
        $crate::emac_static_async!($emac, $state, 10, 10, 1600);
    };
    ($emac:ident, $state:ident, $rx:expr, $tx:expr, $buf:expr) => {
        #[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".dram1"))]
        static $emac: static_cell::StaticCell<$crate::Emac<$rx, $tx, $buf>> =
            static_cell::StaticCell::new();
        static $state: $crate::AsyncEmacState = $crate::AsyncEmacState::new();
    };
}

/// Declare static embassy-net driver state and stack resources.
///
/// This macro requires the `embassy-net` feature plus `embassy_net` and
/// `static_cell` dependencies in your application crate.
///
/// # Examples
///
/// ```ignore
/// ph_esp32_mac::embassy_net_statics!(EMAC, EMAC_STATE, RESOURCES, 10, 10, 1600, 4);
/// ```
#[cfg(feature = "embassy-net")]
#[macro_export]
macro_rules! embassy_net_statics {
    ($emac:ident, $state:ident, $resources:ident, $rx:expr, $tx:expr, $buf:expr, $res:expr) => {
        #[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".dram1"))]
        static $emac: static_cell::StaticCell<$crate::Emac<$rx, $tx, $buf>> =
            static_cell::StaticCell::new();
        static $state: $crate::EmbassyEmacState =
            $crate::EmbassyEmacState::new(embassy_net_driver::LinkState::Down);
        static $resources: static_cell::StaticCell<embassy_net::StackResources<$res>> =
            static_cell::StaticCell::new();
    };
}

/// Create an embassy-net driver from a static EMAC pointer and state.
///
/// This macro performs the unsafe pointer cast for you. The caller must ensure
/// the EMAC pointer is valid for the duration of the program.
///
/// # Examples
///
/// ```ignore
/// let driver = ph_esp32_mac::embassy_net_driver!(emac_ptr, &EMAC_STATE);
/// ```
#[cfg(feature = "embassy-net")]
#[macro_export]
macro_rules! embassy_net_driver {
    ($emac_ptr:expr, $state:expr) => {{
        // SAFETY: The caller guarantees that the EMAC pointer is valid for the program lifetime.
        unsafe { $crate::EmbassyEmac::new(&mut *($emac_ptr), $state) }
    }};
}

/// Create an embassy-net stack and runner with static resources.
///
/// This macro reduces boilerplate for `embassy_net::new`.
///
/// # Examples
///
/// ```ignore
/// let (stack, runner) = ph_esp32_mac::embassy_net_stack!(
///     driver,
///     RESOURCES,
///     Config::default(),
///     seed
/// );
/// ```
#[cfg(feature = "embassy-net")]
#[macro_export]
macro_rules! embassy_net_stack {
    ($driver:expr, $resources:ident, $config:expr, $seed:expr) => {{
        let resources = $resources.init(embassy_net::StackResources::new());
        embassy_net::new($driver, $config, resources, $seed)
    }};
}
