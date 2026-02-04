//! ISR-Safe Synchronization Wrappers
//!
//! This module provides interrupt-safe wrappers for the EMAC driver using
//! the `critical-section` crate. These wrappers allow safe access to the
//! EMAC from both normal code and interrupt handlers.
//!
//! # Types
//!
//! - [`SharedEmac`] - Synchronous ISR-safe EMAC wrapper
//! - [`AsyncSharedEmac`] - Async-capable ISR-safe EMAC wrapper (requires `async` feature)
//!
//! # When to Use
//!
//! Use `SharedEmac` when you need to:
//! - Access the EMAC from interrupt handlers
//! - Share the EMAC between multiple contexts safely
//! - Avoid `unsafe` in your application code
//!
//! Use `AsyncSharedEmac` when you need both:
//! - ISR-safe access patterns (like `SharedEmac`)
//! - Async/await support for non-blocking I/O
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

use crate::mac::Emac;
use crate::sync_primitives::CriticalSectionCell;

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
    inner: CriticalSectionCell<Emac<RX_BUFS, TX_BUFS, BUF_SIZE>>,
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
            inner: CriticalSectionCell::new(Emac::new()),
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
        self.inner.with(f)
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
        self.inner.try_with(f)
    }
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Default
    for SharedEmac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// AsyncSharedEmac
// =============================================================================

/// ISR-safe async-capable EMAC wrapper
///
/// This wrapper combines the ISR-safety of [`SharedEmac`] with async/await
/// capabilities. It provides both synchronous access via `with()` and
/// async methods that yield when waiting for I/O.
///
/// # When to Use
///
/// Use `AsyncSharedEmac` when you need:
/// - Safe access from both main code and interrupt handlers
/// - Async/await support for non-blocking I/O
/// - Integration with async executors (embassy, etc.)
///
/// # Example
///
/// ```ignore
/// use ph_esp32_mac::sync::AsyncSharedEmac;
/// use ph_esp32_mac::EmacConfig;
///
/// static EMAC: AsyncSharedEmac<10, 10, 1600> = AsyncSharedEmac::new();
///
/// async fn ethernet_task() {
///     let mut buf = [0u8; 1600];
///     
///     loop {
///         // Async receive - yields until frame available
///         let len = EMAC.receive_async(&mut buf).await.unwrap();
///         
///         // Process and respond
///         let response = process(&buf[..len]);
///         EMAC.transmit_async(&response).await.unwrap();
///     }
/// }
///
/// #[interrupt]
/// fn ETH_MAC() {
///     // Wake async tasks when hardware events occur
///     ph_esp32_mac::asynch::async_interrupt_handler();
/// }
/// ```
pub struct AsyncSharedEmac<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    inner: CriticalSectionCell<Emac<RX_BUFS, TX_BUFS, BUF_SIZE>>,
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    AsyncSharedEmac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    /// Create a new async shared EMAC instance
    ///
    /// This is a const function suitable for static initialization.
    ///
    /// # Example
    ///
    /// ```ignore
    /// static EMAC: AsyncSharedEmac<10, 10, 1600> = AsyncSharedEmac::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            inner: CriticalSectionCell::new(Emac::new()),
        }
    }

    /// Execute a closure with exclusive access to the EMAC (synchronous)
    ///
    /// This is identical to [`SharedEmac::with()`] - it acquires a critical
    /// section and executes the closure with a mutable reference to the EMAC.
    ///
    /// # Arguments
    ///
    /// * `f` - Closure that receives `&mut Emac` and returns a value
    ///
    /// # Returns
    ///
    /// The return value of the closure
    #[inline]
    pub fn with<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Emac<RX_BUFS, TX_BUFS, BUF_SIZE>) -> R,
    {
        self.inner.with(f)
    }

    /// Try to execute a closure, returning `None` if already borrowed
    #[inline]
    pub fn try_with<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Emac<RX_BUFS, TX_BUFS, BUF_SIZE>) -> R,
    {
        self.inner.try_with(f)
    }

    /// Receive a frame asynchronously
    ///
    /// This method yields until a frame is available, using the global
    /// `RX_WAKER` to be notified by the interrupt handler.
    ///
    /// # Arguments
    ///
    /// * `buffer` - Buffer to receive the frame into
    ///
    /// # Returns
    ///
    /// Returns the frame length on success.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Frame had CRC/length errors
    /// - Buffer too small for frame
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut buf = [0u8; 1600];
    /// let len = EMAC.receive_async(&mut buf).await?;
    /// process(&buf[..len]);
    /// ```
    pub async fn receive_async(&self, buffer: &mut [u8]) -> crate::Result<usize> {
        use crate::asynch::RX_WAKER;
        use core::future::poll_fn;
        use core::task::Poll;

        poll_fn(|cx| {
            // Check if a frame is available
            let result = self.inner.with(|emac| {
                if emac.rx_available() {
                    Some(emac.receive(buffer))
                } else {
                    None
                }
            });

            match result {
                Some(Ok(len)) => Poll::Ready(Ok(len)),
                Some(Err(e)) => Poll::Ready(Err(e)),
                None => {
                    // No frame available, register waker
                    RX_WAKER.register(cx.waker());
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// Transmit a frame asynchronously
    ///
    /// This method yields until a TX buffer is available and the frame has
    /// been queued for transmission.
    ///
    /// # Arguments
    ///
    /// * `data` - The frame data to transmit (must include Ethernet header)
    ///
    /// # Returns
    ///
    /// Returns the number of bytes submitted on success.
    ///
    /// # Errors
    ///
    /// Returns an error if frame is too large for TX buffer.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let packet = build_packet();
    /// let len = EMAC.transmit_async(&packet).await?;
    /// ```
    pub async fn transmit_async(&self, data: &[u8]) -> crate::Result<usize> {
        use crate::asynch::TX_WAKER;
        use core::future::poll_fn;
        use core::task::Poll;

        poll_fn(|cx| {
            // Check if TX is ready
            let result = self.inner.with(|emac| {
                if emac.tx_ready() {
                    Some(emac.transmit(data))
                } else {
                    None
                }
            });

            match result {
                Some(Ok(len)) => Poll::Ready(Ok(len)),
                Some(Err(e)) => Poll::Ready(Err(e)),
                None => {
                    // No TX slot available, register waker
                    TX_WAKER.register(cx.waker());
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// Check if the EMAC has received frames waiting
    ///
    /// This is a synchronous check that does not yield.
    pub fn rx_available(&self) -> bool {
        self.inner.with(|emac| emac.rx_available())
    }

    /// Check if the EMAC can accept a frame for transmission
    ///
    /// This is a synchronous check that does not yield.
    pub fn tx_ready(&self) -> bool {
        self.inner.with(|emac| emac.tx_ready())
    }
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Default
    for AsyncSharedEmac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    fn default() -> Self {
        Self::new()
    }
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

/// Default async shared EMAC configuration
pub type AsyncSharedEmacDefault = AsyncSharedEmac<10, 10, 1600>;

/// Small async shared EMAC configuration
pub type AsyncSharedEmacSmall = AsyncSharedEmac<4, 4, 1600>;

/// Large async shared EMAC configuration
pub type AsyncSharedEmacLarge = AsyncSharedEmac<16, 16, 1600>;

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

    // =========================================================================
    // AsyncSharedEmac Construction Tests
    // =========================================================================

    #[test]
    fn test_async_shared_emac_new() {
        static _EMAC: AsyncSharedEmac<10, 10, 1600> = AsyncSharedEmac::new();
    }

    #[test]
    fn test_async_shared_emac_default() {
        let _emac: AsyncSharedEmacDefault = AsyncSharedEmac::default();
    }

    #[test]
    fn test_async_shared_emac_type_aliases() {
        let _small: AsyncSharedEmacSmall = AsyncSharedEmac::new();
        let _large: AsyncSharedEmacLarge = AsyncSharedEmac::new();
    }

    // =========================================================================
    // AsyncSharedEmac Sync Access Tests
    // =========================================================================

    #[test]
    fn test_async_shared_emac_with_returns_value() {
        let shared: AsyncSharedEmacDefault = AsyncSharedEmac::new();
        let result = shared.with(|_emac| 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_async_shared_emac_with_can_read_state() {
        let shared: AsyncSharedEmacDefault = AsyncSharedEmac::new();
        let state = shared.with(|emac| emac.state());
        assert_eq!(state, State::Uninitialized);
    }

    #[test]
    fn test_async_shared_emac_try_with_returns_some() {
        let shared: AsyncSharedEmacDefault = AsyncSharedEmac::new();
        let result = shared.try_with(|_emac| 123);
        assert_eq!(result, Some(123));
    }

    // =========================================================================
    // AsyncSharedEmac Status Checks
    // =========================================================================

    #[test]
    fn test_async_shared_emac_rx_available_uninitialized() {
        let shared: AsyncSharedEmacDefault = AsyncSharedEmac::new();
        // Uninitialized EMAC returns false for rx_available
        assert!(!shared.rx_available());
    }

    #[test]
    fn test_async_shared_emac_tx_ready_uninitialized() {
        let shared: AsyncSharedEmacDefault = AsyncSharedEmac::new();
        // Uninitialized EMAC with fresh descriptors - they are not owned by DMA,
        // so tx_ready returns true (descriptors are available)
        assert!(shared.tx_ready());
    }

    #[test]
    fn test_static_async_shared_emac() {
        static SHARED: AsyncSharedEmac<10, 10, 1600> = AsyncSharedEmac::new();

        let state = SHARED.with(|emac| emac.state());
        assert_eq!(state, State::Uninitialized);
    }
}
