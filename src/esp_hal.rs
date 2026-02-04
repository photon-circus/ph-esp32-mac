//! esp-hal Integration Module
//!
//! This module provides ergonomic integration with `esp-hal` when the `esp-hal` feature
//! is enabled. It offers:
//!
//! - [`EmacExt`]: Extension trait for interrupt handler registration
//! - [`emac_isr!`]: Macro for defining EMAC interrupt handlers with esp-hal semantics
//! - Type aliases and re-exports for common esp-hal types
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
//!         let status = emac.read_interrupt_status();
//!         emac.clear_interrupts(status);
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
//!     emac.init(config, &mut delay).unwrap();
//!     
//!     // Enable interrupt with esp-hal
//!     emac.enable_emac_interrupt(EMAC_IRQ);
//!     
//!     emac.start().unwrap();
//! }
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
pub use esp_hal::delay::Delay;
pub use esp_hal::interrupt::{self, InterruptHandler, Priority};
pub use esp_hal::peripherals::Interrupt;

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
    ///         let status = emac.read_interrupt_status();
    ///         emac.clear_interrupts(status);
    ///     });
    /// }
    ///
    /// emac.enable_emac_interrupt(emac_handler);
    /// ```
    fn enable_emac_interrupt(&mut self, handler: InterruptHandler);

    /// Disable the EMAC interrupt.
    ///
    /// Call this before reconfiguring the EMAC or when shutting down.
    fn disable_emac_interrupt(&mut self);
}

impl<const RX: usize, const TX: usize, const BUF: usize> EmacExt
    for crate::Emac<RX, TX, BUF>
{
    fn enable_emac_interrupt(&mut self, handler: InterruptHandler) {
        // Disable on other cores if multi-core
        #[cfg(multi_core)]
        for core in esp_hal::system::Cpu::other() {
            esp_hal::interrupt::disable(core, EMAC_INTERRUPT);
        }

        // Bind and enable
        // SAFETY: We're the only EMAC driver, so we own this interrupt binding
        unsafe {
            esp_hal::interrupt::bind_interrupt(EMAC_INTERRUPT, handler.handler());
        }
        esp_hal::interrupt::enable(EMAC_INTERRUPT, handler.priority())
            .expect("Failed to enable EMAC interrupt");
    }

    fn disable_emac_interrupt(&mut self) {
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
///         let status = emac.read_interrupt_status();
///         emac.clear_interrupts(status);
///     });
/// });
///
/// // In main, enable the interrupt
/// fn main() {
///     // ... init emac ...
///     unsafe { &mut *EMAC.get() }.enable_emac_interrupt(EMAC_HANDLER);
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

/// Helper struct for managing EMAC with esp-hal peripheral ownership.
///
/// This wrapper provides a path toward full esp-hal integration by tracking
/// peripheral ownership. For now, it's a marker that documents the intended
/// future API.
///
/// # Future API (when esp-hal adds EMAC support)
///
/// ```ignore
/// use esp_hal::peripherals::{EMAC_DMA, EMAC_MAC, EMAC_EXT};
/// use ph_esp32_mac::esp_hal::EspHalEmac;
///
/// let emac = EspHalEmac::new(
///     peripherals.EMAC_DMA,
///     peripherals.EMAC_MAC,
///     peripherals.EMAC_EXT,
///     config,
/// );
/// ```
pub struct EspHalEmac<'a, const RX: usize, const TX: usize, const BUF: usize> {
    inner: &'a mut crate::Emac<RX, TX, BUF>,
    // Future: add peripheral ownership tokens
    // _dma: EMAC_DMA,
    // _mac: EMAC_MAC,
    // _ext: EMAC_EXT,
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize> EspHalEmac<'a, RX, TX, BUF> {
    /// Create a new esp-hal EMAC wrapper.
    ///
    /// This is a transitional API. In the future, this will take peripheral
    /// ownership tokens from esp-hal.
    pub fn new(inner: &'a mut crate::Emac<RX, TX, BUF>) -> Self {
        Self { inner }
    }

    /// Get a reference to the underlying EMAC driver.
    pub fn inner(&self) -> &crate::Emac<RX, TX, BUF> {
        self.inner
    }

    /// Get a mutable reference to the underlying EMAC driver.
    pub fn inner_mut(&mut self) -> &mut crate::Emac<RX, TX, BUF> {
        self.inner
    }

    /// Initialize the EMAC with esp-hal delay.
    ///
    /// Uses `esp_hal::delay::Delay` for timing operations.
    pub fn init_with_delay(
        &mut self,
        config: crate::EmacConfig,
    ) -> crate::Result<()> {
        let mut delay = Delay::new();
        self.inner.init(config, &mut delay)
    }
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize> core::ops::Deref
    for EspHalEmac<'a, RX, TX, BUF>
{
    type Target = crate::Emac<RX, TX, BUF>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a, const RX: usize, const TX: usize, const BUF: usize> core::ops::DerefMut
    for EspHalEmac<'a, RX, TX, BUF>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

// Also implement EmacExt for the wrapper
impl<'a, const RX: usize, const TX: usize, const BUF: usize> EmacExt
    for EspHalEmac<'a, RX, TX, BUF>
{
    fn enable_emac_interrupt(&mut self, handler: InterruptHandler) {
        self.inner.enable_emac_interrupt(handler);
    }

    fn disable_emac_interrupt(&mut self) {
        self.inner.disable_emac_interrupt();
    }
}

#[cfg(test)]
mod tests {
    // Tests would require esp-hal environment
}
