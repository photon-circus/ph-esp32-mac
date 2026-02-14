//! esp-hal integration module.
#![cfg_attr(docsrs, doc(cfg(feature = "esp-hal")))]
//!
//! This module provides ergonomic integration with `esp-hal` when the `esp-hal` feature
//! is enabled. It offers:
//!
//! - [`EmacExt`]: Extension trait for interrupt handler registration
//! - [`emac_isr!`]: Macro for defining EMAC interrupt handlers with esp-hal semantics
//! - [`emac_async_isr!`]: Macro for defining EMAC async ISR handlers
//! - [`EmacBuilder`]: Builder for minimal-boilerplate esp-hal bring-up
//! - [`EmacPhyBundle`]: Convenience wrapper for PHY + MDIO bring-up
//! - [`Wt32Eth01`]: Board helper for the canonical WT32-ETH01 bring-up (ESP32 only)
//! - Re-exports for common esp-hal types
//!
//! # Usage
//!
//! ```ignore
//! use ph_esp32_mac::{Emac, EmacConfig};
//! use ph_esp32_mac::esp_hal::{emac_isr, EmacExt, Interrupt, Priority};
//! use esp_hal::delay::Delay;
//!
//! // Define interrupt handler using esp-hal-style macro
//! emac_isr!(EMAC_IRQ, Priority::Priority1, {
//!     EMAC.with(|emac| {
//!         let status = emac.handle_interrupt();
//!         
//!         if status.rx_complete() {
//!             // Signal RX task...
//!         }
//!     });
//! });
//!
//! fn main() {
//!     let mut delay = Delay::new();
//!     let emac = unsafe { &mut EMAC };
//!     
//!     EmacBuilder::new(emac)
//!         .with_config(config)
//!         .init(&mut delay)
//!         .unwrap();
//!     
//!     // Enable interrupt with esp-hal
//!     emac.bind_interrupt(EMAC_IRQ);
//!     
//!     emac.start().unwrap();
//! }
//! ```
//!
//! # PHY Bring-up Helper
//!
//! ```ignore
//! use ph_esp32_mac::esp_hal::EmacPhyBundle;
//!
//! let mut emac_phy = EmacPhyBundle::new(
//!     emac,
//!     Lan8720a::new(PHY_ADDR),
//!     MdioController::new(Delay::new()),
//! );
//! emac_phy.init_phy().unwrap();
//! let _status = emac_phy.wait_link_up(&mut delay, 10_000, 200).unwrap();
//! ```
//!
//! # WT32-ETH01 Canonical Happy Path (ESP32)
//!
//! This is the recommended esp-hal bring-up path for this crate.
//!
//! ```ignore
//! use esp_hal::delay::Delay;
//! use ph_esp32_mac::esp_hal::{EmacBuilder, EmacPhyBundle, Wt32Eth01};
//!
//! let mut delay = Delay::new();
//! let emac = unsafe { &mut EMAC };
//!
//! EmacBuilder::wt32_eth01_with_mac(emac, [0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
//!     .init(&mut delay)
//!     .unwrap();
//!
//! let mut emac_phy = EmacPhyBundle::wt32_eth01_lan8720a(emac, Delay::new());
//! emac_phy.init_phy().unwrap();
//! let _status = emac_phy.wait_link_up(&mut delay, 10_000, 200).unwrap();
//! ```
//!
//! # Async Usage (per-instance wakers)
//!
//! ```ignore
//! use ph_esp32_mac::{AsyncEmacExt, AsyncEmacState};
//! use ph_esp32_mac::esp_hal::{emac_async_isr, EmacExt, Priority};
//!
//! static ASYNC_STATE: AsyncEmacState = AsyncEmacState::new();
//!
//! emac_async_isr!(EMAC_IRQ, Priority::Priority1, &ASYNC_STATE);
//!
//! // In main:
//! emac.bind_interrupt(EMAC_IRQ);
//!
//! // In an async task:
//! let len = emac.receive_async(&ASYNC_STATE, &mut buffer).await?;
//! ```
//!
//! # Feature Detection
//!
//! This module is only available when the `esp-hal` feature is enabled:
//!
//! ```toml
//! [dependencies]
//! ph-esp32-mac = { version = "0.1", features = ["esp-hal"] }
//! ```

// Re-export esp-hal types for convenience
#[cfg(feature = "esp32")]
#[cfg_attr(docsrs, doc(cfg(feature = "esp32")))]
pub use crate::boards::wt32_eth01::Wt32Eth01;
pub use esp_hal::delay::Delay;
pub use esp_hal::interrupt::{InterruptHandler, Priority};
pub use esp_hal::peripherals::Interrupt;

use embedded_hal::delay::DelayNs;

use crate::driver::error::{ConfigError, IoError};
use crate::hal::mdio::MdioBus;
#[cfg(feature = "esp32")]
use crate::hal::mdio::MdioController;
#[cfg(feature = "esp32")]
use crate::phy::Lan8720a;
use crate::phy::{LinkStatus, PhyDriver};

/// Builder for esp-hal-friendly EMAC initialization.
///
/// This builder reduces boilerplate by bundling common setup steps for
/// esp-hal users while keeping the driver implementation unchanged.
///
/// # Important
///
/// The EMAC must already be placed in its final memory location before
/// calling [`EmacBuilder::init`]. This is required for DMA descriptors.
///
/// # Example
///
/// ```ignore
/// let mut delay = Delay::new();
/// let emac = unsafe { &mut EMAC };
///
/// EmacBuilder::new(emac)
///     .with_config(EmacConfig::rmii_esp32_default())
///     .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
///     .init(&mut delay)
///     .unwrap();
/// ```
pub struct EmacBuilder<'a, const RX: usize, const TX: usize, const BUF: usize> {
    emac: &'a mut crate::Emac<RX, TX, BUF>,
    config: crate::EmacConfig,
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize> EmacBuilder<'a, RX, TX, BUF> {
    /// Create a new esp-hal EMAC builder.
    ///
    /// # Arguments
    ///
    /// * `emac` - EMAC instance already placed in its final memory location
    ///
    /// # Returns
    ///
    /// A builder with ESP32 RMII defaults.
    pub fn new(emac: &'a mut crate::Emac<RX, TX, BUF>) -> Self {
        Self {
            emac,
            config: crate::EmacConfig::rmii_esp32_default(),
        }
    }

    /// Create a WT32-ETH01 builder with board defaults.
    ///
    /// # Arguments
    ///
    /// * `emac` - EMAC instance already placed in its final memory location
    ///
    /// # Returns
    ///
    /// A builder pre-configured for WT32-ETH01.
    #[cfg(feature = "esp32")]
    #[cfg_attr(docsrs, doc(cfg(feature = "esp32")))]
    pub fn wt32_eth01(emac: &'a mut crate::Emac<RX, TX, BUF>) -> Self {
        Self {
            emac,
            config: Wt32Eth01::emac_config(),
        }
    }

    /// Create a WT32-ETH01 builder with a custom MAC address.
    ///
    /// # Arguments
    ///
    /// * `emac` - EMAC instance already placed in its final memory location
    /// * `mac_address` - 6-byte MAC address
    ///
    /// # Returns
    ///
    /// A builder pre-configured for WT32-ETH01.
    #[cfg(feature = "esp32")]
    #[cfg_attr(docsrs, doc(cfg(feature = "esp32")))]
    pub fn wt32_eth01_with_mac(
        emac: &'a mut crate::Emac<RX, TX, BUF>,
        mac_address: [u8; 6],
    ) -> Self {
        Self {
            emac,
            config: Wt32Eth01::emac_config_with_mac(mac_address),
        }
    }

    /// Override the full EMAC configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Complete EMAC configuration
    #[must_use]
    pub const fn with_config(mut self, config: crate::EmacConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the MAC address.
    ///
    /// # Arguments
    ///
    /// * `mac_address` - 6-byte MAC address
    #[must_use]
    pub const fn with_mac_address(mut self, mac_address: [u8; 6]) -> Self {
        self.config = self.config.with_mac_address(mac_address);
        self
    }

    /// Set the RMII clock mode.
    ///
    /// # Arguments
    ///
    /// * `rmii_clock` - RMII clock configuration
    #[must_use]
    pub const fn with_rmii_clock(mut self, rmii_clock: crate::RmiiClockMode) -> Self {
        self.config = self.config.with_rmii_clock(rmii_clock);
        self
    }

    /// Set the RMII clock to an external 50 MHz input on the given GPIO.
    ///
    /// # Arguments
    ///
    /// * `gpio` - GPIO number for the RMII clock input (typically GPIO0)
    #[must_use]
    pub const fn with_rmii_external_clock(mut self, gpio: u8) -> Self {
        self.config = self.config.with_rmii_external_clock(gpio);
        self
    }

    /// Set the RMII clock to an internal 50 MHz output on the given GPIO.
    ///
    /// # Arguments
    ///
    /// * `gpio` - GPIO number for the RMII clock output (GPIO16 or GPIO17)
    #[must_use]
    pub const fn with_rmii_internal_clock(mut self, gpio: u8) -> Self {
        self.config = self.config.with_rmii_internal_clock(gpio);
        self
    }

    /// Initialize the EMAC using an esp-hal delay provider.
    ///
    /// # Arguments
    ///
    /// * `delay` - esp-hal delay provider
    ///
    /// # Returns
    ///
    /// A mutable reference to the initialized EMAC.
    ///
    /// # Errors
    ///
    /// Propagates initialization errors from [`Emac::init`].
    pub fn init(self, delay: &mut Delay) -> crate::Result<&'a mut crate::Emac<RX, TX, BUF>> {
        self.emac.init(self.config, delay)?;
        Ok(self.emac)
    }

    /// Initialize and start the EMAC using an esp-hal delay provider.
    ///
    /// # Arguments
    ///
    /// * `delay` - esp-hal delay provider
    ///
    /// # Returns
    ///
    /// A mutable reference to the initialized EMAC.
    ///
    /// # Errors
    ///
    /// Propagates initialization or start errors.
    pub fn init_and_start(
        self,
        delay: &mut Delay,
    ) -> crate::Result<&'a mut crate::Emac<RX, TX, BUF>> {
        self.emac.init(self.config, delay)?;
        self.emac.start()?;
        Ok(self.emac)
    }
}

/// Convenience wrapper for EMAC + PHY + MDIO bring-up with esp-hal.
///
/// This helper reduces boilerplate by bundling PHY initialization and
/// link-up polling while keeping EMAC ownership explicit.
pub struct EmacPhyBundle<'a, const RX: usize, const TX: usize, const BUF: usize, P, M> {
    emac: &'a mut crate::Emac<RX, TX, BUF>,
    phy: P,
    mdio: M,
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize, P, M>
    EmacPhyBundle<'a, RX, TX, BUF, P, M>
where
    P: PhyDriver,
    M: MdioBus,
{
    /// Create a new EMAC/PHY bundle.
    ///
    /// # Arguments
    ///
    /// * `emac` - Initialized EMAC instance in its final memory location
    /// * `phy` - PHY driver instance
    /// * `mdio` - MDIO bus implementation
    pub fn new(emac: &'a mut crate::Emac<RX, TX, BUF>, phy: P, mdio: M) -> Self {
        Self { emac, phy, mdio }
    }

    /// Borrow the EMAC instance.
    pub fn emac_mut(&mut self) -> &mut crate::Emac<RX, TX, BUF> {
        self.emac
    }

    /// Borrow the PHY instance.
    pub fn phy_mut(&mut self) -> &mut P {
        &mut self.phy
    }

    /// Borrow the MDIO bus.
    pub fn mdio_mut(&mut self) -> &mut M {
        &mut self.mdio
    }

    /// Initialize the PHY.
    ///
    /// # Errors
    ///
    /// Propagates PHY/MDIO errors from the underlying driver.
    pub fn init_phy(&mut self) -> crate::Result<()> {
        self.phy.init(&mut self.mdio)
    }

    /// Read the current link status and apply speed/duplex to the EMAC.
    ///
    /// # Returns
    ///
    /// `Some(LinkStatus)` when link is up, `None` when link is down.
    ///
    /// # Errors
    ///
    /// Propagates PHY/MDIO errors from the underlying driver.
    pub fn link_status(&mut self) -> crate::Result<Option<LinkStatus>> {
        let status = self.phy.link_status(&mut self.mdio)?;
        self.apply_link(status);
        Ok(status)
    }

    /// Poll for link changes and apply speed/duplex to the EMAC.
    ///
    /// # Returns
    ///
    /// `Some(LinkStatus)` when a new link is established, `None` otherwise.
    ///
    /// # Errors
    ///
    /// Propagates PHY/MDIO errors from the underlying driver.
    pub fn poll_link(&mut self) -> crate::Result<Option<LinkStatus>> {
        let status = self.phy.poll_link(&mut self.mdio)?;
        self.apply_link(status);
        Ok(status)
    }

    /// Wait for link-up with a timeout.
    ///
    /// # Arguments
    ///
    /// * `delay` - Delay provider
    /// * `timeout_ms` - Total timeout in milliseconds
    /// * `poll_interval_ms` - Poll interval in milliseconds
    ///
    /// # Returns
    ///
    /// Link status once a link is established.
    ///
    /// # Errors
    ///
    /// Returns [`IoError::Timeout`] if the timeout expires.
    /// Propagates PHY/MDIO errors from the underlying driver.
    pub fn wait_link_up<D: DelayNs>(
        &mut self,
        delay: &mut D,
        timeout_ms: u32,
        poll_interval_ms: u32,
    ) -> crate::Result<LinkStatus> {
        if poll_interval_ms == 0 {
            return Err(ConfigError::InvalidConfig.into());
        }

        if let Some(status) = self.link_status()? {
            return Ok(status);
        }

        let mut elapsed_ms = 0u32;
        while elapsed_ms < timeout_ms {
            delay.delay_ms(poll_interval_ms);
            elapsed_ms = elapsed_ms.saturating_add(poll_interval_ms);

            if let Some(status) = self.link_status()? {
                return Ok(status);
            }
        }

        Err(IoError::Timeout.into())
    }

    /// Initialize the PHY and wait for link-up with a timeout.
    ///
    /// # Arguments
    ///
    /// * `delay` - Delay provider
    /// * `timeout_ms` - Total timeout in milliseconds
    /// * `poll_interval_ms` - Poll interval in milliseconds
    ///
    /// # Returns
    ///
    /// Link status once a link is established.
    ///
    /// # Errors
    ///
    /// Propagates PHY/MDIO errors from the underlying driver.
    /// Returns [`IoError::Timeout`] if the timeout expires.
    pub fn init_and_wait_link_up<D: DelayNs>(
        &mut self,
        delay: &mut D,
        timeout_ms: u32,
        poll_interval_ms: u32,
    ) -> crate::Result<LinkStatus> {
        self.init_phy()?;
        self.wait_link_up(delay, timeout_ms, poll_interval_ms)
    }

    /// Consume the bundle and return the parts.
    pub fn into_parts(self) -> (&'a mut crate::Emac<RX, TX, BUF>, P, M) {
        (self.emac, self.phy, self.mdio)
    }

    fn apply_link(&mut self, status: Option<LinkStatus>) {
        if let Some(status) = status {
            self.emac.set_speed(status.speed);
            self.emac.set_duplex(status.duplex);
        }
    }
}

#[cfg(feature = "esp32")]
#[cfg_attr(docsrs, doc(cfg(feature = "esp32")))]
impl<'a, const RX: usize, const TX: usize, const BUF: usize, D>
    EmacPhyBundle<'a, RX, TX, BUF, Lan8720a, MdioController<D>>
where
    D: DelayNs,
{
    /// Create a WT32-ETH01 LAN8720A + MDIO bundle.
    ///
    /// # Arguments
    ///
    /// * `emac` - Initialized EMAC instance in its final memory location
    /// * `delay` - Delay provider for MDIO timeouts
    ///
    /// # Returns
    ///
    /// A bundle configured for WT32-ETH01.
    pub fn wt32_eth01_lan8720a(emac: &'a mut crate::Emac<RX, TX, BUF>, delay: D) -> Self {
        Self::new(emac, Wt32Eth01::lan8720a(), MdioController::new(delay))
    }
}

/// The EMAC peripheral interrupt source.
///
/// On ESP32, the EMAC generates a single combined interrupt for all events
/// (TX complete, RX complete, errors, etc.). Use [`InterruptStatus`] to
/// determine which event(s) triggered the interrupt.
///
/// [`InterruptStatus`]: crate::InterruptStatus
pub const EMAC_INTERRUPT: Interrupt = Interrupt::ETH_MAC;

/// Extension trait for EMAC interrupt management with esp-hal.
///
/// This trait provides ergonomic methods for working with EMAC interrupts
/// using esp-hal's interrupt system.
pub trait EmacExt {
    /// Enable the EMAC interrupt with the given handler.
    ///
    /// This is equivalent to calling:
    /// ```ignore
    /// unsafe { esp_hal::interrupt::bind_interrupt(Interrupt::ETH_MAC, handler.handler()) };
    /// esp_hal::interrupt::enable(Interrupt::ETH_MAC, handler.priority()).unwrap();
    /// ```
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ph_esp32_mac::esp_hal::{EmacExt, Priority};
    ///
    /// #[esp_hal::handler(priority = Priority::Priority1)]
    /// fn emac_handler() {
    ///     EMAC.with(|emac| {
    ///         let _status = emac.handle_interrupt();
    ///     });
    /// }
    ///
    /// emac.bind_interrupt(emac_handler);
    /// ```
    fn bind_interrupt(&mut self, handler: InterruptHandler);

    /// Disable the EMAC interrupt.
    ///
    /// Call this before reconfiguring the EMAC or when shutting down.
    fn disable_interrupt(&mut self);
}

impl<const RX: usize, const TX: usize, const BUF: usize> EmacExt for crate::Emac<RX, TX, BUF> {
    fn bind_interrupt(&mut self, handler: InterruptHandler) {
        // Disable on other cores when present.
        for core in esp_hal::system::Cpu::other() {
            esp_hal::interrupt::disable(core, EMAC_INTERRUPT);
        }

        // Bind and enable
        // SAFETY: We're the only EMAC driver, so we own this interrupt
        // binding. The handler function pointer is valid for the static
        // lifetime because esp-hal handlers are generated as static functions.
        unsafe {
            esp_hal::interrupt::bind_interrupt(EMAC_INTERRUPT, handler.handler());
        }
        esp_hal::interrupt::enable(EMAC_INTERRUPT, handler.priority())
            .expect("Failed to enable EMAC interrupt");
    }

    fn disable_interrupt(&mut self) {
        esp_hal::interrupt::disable(esp_hal::system::Cpu::current(), EMAC_INTERRUPT);
    }
}

/// Macro for defining an EMAC interrupt handler with esp-hal semantics.
///
/// This macro creates an interrupt handler function that follows esp-hal patterns
/// and provides convenient access to the EMAC driver.
///
/// # Parameters
///
/// - `$name`: The name for the handler constant (e.g., `EMAC_HANDLER`)
/// - `$priority`: The interrupt priority (e.g., `Priority::Priority1`)
/// - `$body`: The handler body (has access to `emac` variable)
///
/// # Example
///
/// ```ignore
/// use ph_esp32_mac::{SharedEmac, InterruptStatus};
/// use ph_esp32_mac::esp_hal::{emac_isr, Priority};
///
/// static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();
///
/// // Simple handler
/// emac_isr!(EMAC_HANDLER, Priority::Priority1, {
///     EMAC.with(|emac| {
///         let _status = emac.handle_interrupt();
///     });
/// });
///
/// // In main, enable the interrupt
/// fn main() {
///     // ... init emac ...
///     unsafe { &mut *EMAC.get() }.bind_interrupt(EMAC_HANDLER);
/// }
/// ```
///
/// # Equivalent Code
///
/// The macro expands to something like:
///
/// ```ignore
/// #[esp_hal::handler(priority = $priority)]
/// fn __emac_isr_internal() {
///     $body
/// }
/// const $name: InterruptHandler = __emac_isr_internal;
/// ```
#[macro_export]
macro_rules! emac_isr {
    ($name:ident, $priority:expr, $body:block) => {
        #[allow(non_upper_case_globals)]
        const $name: $crate::esp_hal::InterruptHandler = {
            #[esp_hal::handler(priority = $priority)]
            fn __emac_isr_internal() {
                $body
            }
            __emac_isr_internal
        };
    };
}

/// Macro for defining an EMAC async interrupt handler.
///
/// This macro wires the ISR to [`async_interrupt_handler`] using a static
/// [`AsyncEmacState`], minimizing boilerplate for async usage.
///
/// # Parameters
///
/// - `$name`: The name for the handler constant (e.g., `EMAC_ASYNC_IRQ`)
/// - `$priority`: The interrupt priority (e.g., `Priority::Priority1`)
/// - `$state`: Reference to a static [`AsyncEmacState`]
///
/// # Example
///
/// ```ignore
/// use ph_esp32_mac::{AsyncEmacState, AsyncEmacExt};
/// use ph_esp32_mac::esp_hal::{emac_async_isr, EmacExt, Priority};
///
/// static ASYNC_STATE: AsyncEmacState = AsyncEmacState::new();
///
/// emac_async_isr!(EMAC_IRQ, Priority::Priority1, &ASYNC_STATE);
///
/// // In main:
/// emac.bind_interrupt(EMAC_IRQ);
/// ```
#[macro_export]
macro_rules! emac_async_isr {
    ($name:ident, $priority:expr, $state:expr) => {
        #[allow(non_upper_case_globals)]
        const $name: $crate::esp_hal::InterruptHandler = {
            #[esp_hal::handler(priority = $priority)]
            fn __emac_async_isr_internal() {
                $crate::async_interrupt_handler($state);
            }
            __emac_async_isr_internal
        };
    };
}

#[cfg(test)]
mod tests {
    // Tests would require esp-hal environment
}
