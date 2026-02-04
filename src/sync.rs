//! ISR-Safe Synchronization Wrappers
//!
//! This module provides interrupt-safe wrappers for the EMAC driver using
//! the `critical-section` crate. These wrappers allow safe access to the
//! EMAC from both normal code and interrupt handlers.
//!
//! # When to Use
//!
//! Use `SharedEmac` when you need to:
//! - Access the EMAC from interrupt handlers
//! - Share the EMAC between multiple contexts safely
//! - Avoid `unsafe` in your application code
//!
//! For single-context use (no interrupts accessing EMAC), the regular
//! `static mut EMAC` pattern is simpler and has no overhead.
//!
//! # Example
//!
//! ```ignore
//! use esp32_emac::sync::SharedEmac;
//! use esp32_emac::EmacConfig;
//!
//! // Static allocation - safe!
//! static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();
//!
//! fn main() {
//!     // Initialize within critical section
//!     EMAC.with(|emac| {
//!         emac.init(EmacConfig::default()).unwrap();
//!         emac.start().unwrap();
//!     });
//!
//!     loop {
//!         // Transmit within critical section
//!         EMAC.with(|emac| {
//!             let data = b"Hello, Ethernet!";
//!             emac.transmit(data).ok();
//!         });
//!     }
//! }
//!
//! #[interrupt]
//! fn EMAC_IRQ() {
//!     // Safe access from ISR - interrupts disabled during access
//!     EMAC.with(|emac| {
//!         let status = emac.read_interrupt_status();
//!         emac.clear_interrupts(status);
//!         
//!         if status.rx_complete {
//!             // Handle received frames
//!             while let Ok(frame) = emac.receive() {
//!                 // Process frame...
//!             }
//!         }
//!     });
//! }
//! ```
//!
//! # Implementation Note
//!
//! The critical section implementation is provided by the HAL crate
//! (e.g., `esp-hal`). You must enable the appropriate feature:
//!
//! ```toml
//! [dependencies]
//! esp-hal = { version = "...", features = ["critical-section"] }
//! ```
//!
//! For ESP32, this typically disables interrupts on the current core.
//! On dual-core configurations, it also acquires a hardware spinlock.

use core::cell::RefCell;
use critical_section::Mutex;

use crate::mac::Emac;

// =============================================================================
// SharedEmac
// =============================================================================

/// ISR-safe EMAC wrapper using critical sections
///
/// This wrapper allows safe access to the EMAC from both main code and
/// interrupt handlers. All access goes through `critical_section::with()`,
/// which disables interrupts for the duration of the closure.
///
/// # Performance Considerations
///
/// - Each `with()` call disables/enables interrupts
/// - Keep critical sections short to minimize interrupt latency
/// - For high-throughput applications, consider batching operations
///
/// # Example
///
/// ```ignore
/// static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();
///
/// // Access the EMAC safely
/// EMAC.with(|emac| {
///     emac.transmit(&data).ok();
/// });
/// ```
pub struct SharedEmac<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    inner: Mutex<RefCell<Emac<RX_BUFS, TX_BUFS, BUF_SIZE>>>,
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    SharedEmac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    /// Create a new shared EMAC instance
    ///
    /// This is a const function suitable for static initialization.
    ///
    /// # Example
    ///
    /// ```ignore
    /// static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(RefCell::new(Emac::new())),
        }
    }

    /// Execute a closure with exclusive access to the EMAC
    ///
    /// This function acquires a critical section (disables interrupts),
    /// then executes the closure with a mutable reference to the EMAC.
    ///
    /// # Arguments
    ///
    /// * `f` - Closure that receives `&mut Emac` and returns a value
    ///
    /// # Returns
    ///
    /// The return value of the closure
    ///
    /// # Performance
    ///
    /// Interrupts are disabled for the duration of the closure.
    /// Keep the closure short to minimize interrupt latency.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = EMAC.with(|emac| {
    ///     emac.receive()
    /// });
    /// ```
    #[inline]
    pub fn with<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Emac<RX_BUFS, TX_BUFS, BUF_SIZE>) -> R,
    {
        critical_section::with(|cs| {
            let mut emac = self.inner.borrow_ref_mut(cs);
            f(&mut emac)
        })
    }

    /// Try to execute a closure, returning `None` if the EMAC is already borrowed
    ///
    /// This is useful in scenarios where you want to avoid blocking if
    /// another context is currently using the EMAC. Note that since this
    /// uses critical sections, it will still disable interrupts.
    ///
    /// In practice, with proper critical section usage, this should always
    /// succeed (return `Some`), but this method provides a non-panicking
    /// alternative to `with()` for defensive programming.
    ///
    /// # Returns
    ///
    /// - `Some(result)` if the closure was executed
    /// - `None` if the EMAC was already borrowed (should not happen with proper use)
    #[inline]
    pub fn try_with<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Emac<RX_BUFS, TX_BUFS, BUF_SIZE>) -> R,
    {
        critical_section::with(|cs| {
            // try_borrow_mut returns Option<RefMut>, avoiding panic if already borrowed
            self.inner.borrow(cs).try_borrow_mut().ok().map(|mut emac| f(&mut emac))
        })
    }
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Default
    for SharedEmac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    fn default() -> Self {
        Self::new()
    }
}

// Safety: SharedEmac uses critical sections to protect all access,
// making it safe to share across threads/interrupt contexts.
unsafe impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Sync
    for SharedEmac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
}

// =============================================================================
// Type Aliases
// =============================================================================

/// Default shared EMAC configuration (10 RX, 10 TX, 1600 byte buffers)
pub type SharedEmacDefault = SharedEmac<10, 10, 1600>;

/// Small shared EMAC configuration for memory-constrained systems
pub type SharedEmacSmall = SharedEmac<4, 4, 1600>;

/// Large shared EMAC configuration for high-throughput applications
pub type SharedEmacLarge = SharedEmac<16, 16, 1600>;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::State;

    // =========================================================================
    // Construction Tests
    // =========================================================================

    #[test]
    fn test_shared_emac_new() {
        // Just verify const construction works
        static _EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();
    }

    #[test]
    fn test_shared_emac_default() {
        let _emac: SharedEmacDefault = SharedEmac::default();
    }

    #[test]
    fn test_shared_emac_small_type_alias() {
        let _emac: SharedEmacSmall = SharedEmac::new();
    }

    #[test]
    fn test_shared_emac_large_type_alias() {
        let _emac: SharedEmacLarge = SharedEmac::new();
    }

    // =========================================================================
    // with() Tests
    // =========================================================================

    #[test]
    fn test_shared_emac_with_returns_value() {
        let shared: SharedEmacDefault = SharedEmac::new();
        let result = shared.with(|_emac| 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_shared_emac_with_can_read_state() {
        let shared: SharedEmacDefault = SharedEmac::new();
        let state = shared.with(|emac| emac.state());
        assert_eq!(state, State::Uninitialized);
    }

    #[test]
    fn test_shared_emac_with_closure_executed() {
        let shared: SharedEmacDefault = SharedEmac::new();
        let mut executed = false;
        shared.with(|_emac| {
            executed = true;
        });
        assert!(executed);
    }

    // =========================================================================
    // try_with() Tests
    // =========================================================================

    #[test]
    fn test_shared_emac_try_with_returns_some() {
        let shared: SharedEmacDefault = SharedEmac::new();
        let result = shared.try_with(|_emac| 123);
        assert_eq!(result, Some(123));
    }

    #[test]
    fn test_shared_emac_try_with_can_read_state() {
        let shared: SharedEmacDefault = SharedEmac::new();
        let state = shared.try_with(|emac| emac.state());
        assert_eq!(state, Some(State::Uninitialized));
    }

    // =========================================================================
    // Multiple Access Tests
    // =========================================================================

    #[test]
    fn test_shared_emac_multiple_with_calls() {
        let shared: SharedEmacDefault = SharedEmac::new();

        let r1 = shared.with(|_emac| 1);
        let r2 = shared.with(|_emac| 2);
        let r3 = shared.with(|_emac| 3);

        assert_eq!(r1, 1);
        assert_eq!(r2, 2);
        assert_eq!(r3, 3);
    }

    #[test]
    fn test_shared_emac_interleaved_with_try_with() {
        let shared: SharedEmacDefault = SharedEmac::new();

        let r1 = shared.with(|_emac| 1);
        let r2 = shared.try_with(|_emac| 2);
        let r3 = shared.with(|_emac| 3);

        assert_eq!(r1, 1);
        assert_eq!(r2, Some(2));
        assert_eq!(r3, 3);
    }

    // =========================================================================
    // Static Allocation Tests
    // =========================================================================

    #[test]
    fn test_static_shared_emac() {
        static SHARED: SharedEmac<10, 10, 1600> = SharedEmac::new();

        let state = SHARED.with(|emac| emac.state());
        assert_eq!(state, State::Uninitialized);
    }
}
