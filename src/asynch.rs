//! Async Support Module
//!
//! This module provides async/await support for the EMAC driver using wakers.
//! Enable with the `async` feature flag.
//!
//! # Architecture
//!
//! The async implementation uses static [`AtomicWaker`]s that are woken from
//! the EMAC interrupt handler. This allows efficient async I/O without busy-waiting.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        User Code                            │
//! │   let len = emac.receive_async(&mut buf).await?;            │
//! │   emac.transmit_async(&packet).await?;                      │
//! └─────────────────────────────────────────────────────────────┘
//!                             │
//!                             ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    RxFuture / TxFuture                      │
//! │   - Polls hardware for completion                           │
//! │   - Registers waker with static AtomicWaker                 │
//! │   - Returns Poll::Pending if not ready                      │
//! └─────────────────────────────────────────────────────────────┘
//!                             │
//!                             ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    ISR (async_interrupt_handler)            │
//! │   - Reads InterruptStatus                                   │
//! │   - Wakes RX_WAKER if rx_complete                           │
//! │   - Wakes TX_WAKER if tx_complete                           │
//! │   - Clears handled interrupts                               │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use ph_esp32_mac::{Emac, EmacConfig};
//! use ph_esp32_mac::asynch::AsyncEmacExt;
//!
//! static mut EMAC: Emac<10, 10, 1600> = Emac::new();
//!
//! async fn ethernet_task() {
//!     let emac = unsafe { &mut EMAC };
//!     let mut buf = [0u8; 1600];
//!     
//!     loop {
//!         // Async receive - yields until frame available
//!         let len = emac.receive_async(&mut buf).await.unwrap();
//!         
//!         // Process frame...
//!         let response = process(&buf[..len]);
//!         
//!         // Async transmit - yields until TX slot available
//!         emac.transmit_async(&response).await.unwrap();
//!     }
//! }
//!
//! // In ISR (required for async to work!)
//! #[interrupt]
//! fn ETH_MAC() {
//!     ph_esp32_mac::asynch::async_interrupt_handler();
//! }
//! ```
//!
//! # With esp-hal Feature
//!
//! When combined with the `esp-hal` feature, you can use the handler macro:
//!
//! ```ignore
//! use ph_esp32_mac::esp_hal::{emac_isr, Priority, EmacExt};
//!
//! emac_isr!(ASYNC_EMAC_HANDLER, Priority::Priority1, {
//!     ph_esp32_mac::asynch::async_interrupt_handler();
//! });
//!
//! fn main() {
//!     // ...
//!     emac.enable_emac_interrupt(ASYNC_EMAC_HANDLER);
//! }
//! ```

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use crate::{Emac, Error, InterruptStatus, IoError, Result};

// =============================================================================
// AtomicWaker Implementation
// =============================================================================

/// A thread-safe, interrupt-safe waker storage.
///
/// This is similar to `embassy_sync::waitqueue::AtomicWaker` but works without
/// external dependencies. Uses critical-section for safe access.
pub struct AtomicWaker {
    waker: critical_section::Mutex<core::cell::RefCell<Option<Waker>>>,
}

impl AtomicWaker {
    /// Create a new empty AtomicWaker.
    pub const fn new() -> Self {
        Self {
            waker: critical_section::Mutex::new(core::cell::RefCell::new(None)),
        }
    }

    /// Register a waker to be woken later.
    ///
    /// Overwrites any previously registered waker.
    pub fn register(&self, waker: &Waker) {
        critical_section::with(|cs| {
            let mut slot = self.waker.borrow_ref_mut(cs);
            match &*slot {
                Some(existing) if existing.will_wake(waker) => {
                    // Same waker, no action needed
                }
                _ => {
                    *slot = Some(waker.clone());
                }
            }
        });
    }

    /// Wake the registered waker, if any.
    ///
    /// This clears the stored waker after waking.
    #[inline]
    pub fn wake(&self) {
        let waker = critical_section::with(|cs| self.waker.borrow_ref_mut(cs).take());
        if let Some(w) = waker {
            w.wake();
        }
    }

    /// Check if a waker is registered.
    pub fn is_registered(&self) -> bool {
        critical_section::with(|cs| self.waker.borrow_ref(cs).is_some())
    }
}

impl Default for AtomicWaker {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: AtomicWaker uses critical_section for synchronization
unsafe impl Send for AtomicWaker {}
unsafe impl Sync for AtomicWaker {}

// =============================================================================
// Static Wakers
// =============================================================================

/// Waker for RX complete events.
pub static RX_WAKER: AtomicWaker = AtomicWaker::new();

/// Waker for TX complete events.
pub static TX_WAKER: AtomicWaker = AtomicWaker::new();

/// Waker for error events (underflow, overflow, bus error).
pub static ERR_WAKER: AtomicWaker = AtomicWaker::new();

// =============================================================================
// Interrupt Handler
// =============================================================================

/// Async-aware interrupt handler for EMAC.
///
/// This function should be called from your EMAC interrupt handler when using
/// async operations. It reads the interrupt status, wakes the appropriate
/// wakers, and clears the handled interrupts.
///
/// # Example
///
/// ```ignore
/// #[interrupt]
/// fn ETH_MAC() {
///     ph_esp32_mac::asynch::async_interrupt_handler();
/// }
/// ```
///
/// # With esp-hal
///
/// ```ignore
/// use ph_esp32_mac::esp_hal::{emac_isr, Priority};
///
/// emac_isr!(HANDLER, Priority::Priority1, {
///     ph_esp32_mac::asynch::async_interrupt_handler();
/// });
/// ```
#[inline]
pub fn async_interrupt_handler() {
    // Read status
    let status = InterruptStatus::from_raw(crate::register::dma::DmaRegs::status());

    // Wake appropriate wakers based on status
    if status.rx_complete || status.rx_buf_unavailable {
        RX_WAKER.wake();
    }

    if status.tx_complete || status.tx_buf_unavailable {
        TX_WAKER.wake();
    }

    if status.has_error() {
        ERR_WAKER.wake();
        // Also wake RX/TX so they can check for errors
        RX_WAKER.wake();
        TX_WAKER.wake();
    }

    // Clear all handled interrupts (write-1-to-clear)
    crate::register::dma::DmaRegs::set_status(status.to_raw());
}

/// Returns the last interrupt status without clearing.
///
/// Useful for checking error conditions in async code.
#[inline]
pub fn peek_interrupt_status() -> InterruptStatus {
    InterruptStatus::from_raw(crate::register::dma::DmaRegs::status())
}

// =============================================================================
// Futures
// =============================================================================

/// Future for async receive operations.
///
/// This future polls the EMAC for received frames, registering a waker
/// to be notified when a frame becomes available.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct RxFuture<'a, 'b, const RX: usize, const TX: usize, const BUF: usize> {
    emac: &'a mut Emac<RX, TX, BUF>,
    buffer: &'b mut [u8],
}

impl<'a, 'b, const RX: usize, const TX: usize, const BUF: usize> RxFuture<'a, 'b, RX, TX, BUF> {
    /// Create a new RX future.
    pub fn new(emac: &'a mut Emac<RX, TX, BUF>, buffer: &'b mut [u8]) -> Self {
        Self { emac, buffer }
    }
}

impl<const RX: usize, const TX: usize, const BUF: usize> Future
    for RxFuture<'_, '_, RX, TX, BUF>
{
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We're not moving anything out of self
        let this = unsafe { self.get_unchecked_mut() };

        // First check for fatal errors
        let status = peek_interrupt_status();
        if status.fatal_bus_error {
            return Poll::Ready(Err(Error::Io(IoError::FrameError)));
        }

        // Check if a frame is available
        if !this.emac.rx_available() {
            // No frame available, register waker and wait
            RX_WAKER.register(cx.waker());

            // Double-check after registering to avoid race
            if !this.emac.rx_available() {
                return Poll::Pending;
            }
        }

        // Frame is available, try to receive it
        match this.emac.receive(this.buffer) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Future for async transmit operations.
///
/// This future polls the EMAC for an available TX slot, registering a waker
/// to be notified when transmission can proceed.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct TxFuture<'a, 'b, const RX: usize, const TX: usize, const BUF: usize> {
    emac: &'a mut Emac<RX, TX, BUF>,
    data: &'b [u8],
}

impl<'a, 'b, const RX: usize, const TX: usize, const BUF: usize> TxFuture<'a, 'b, RX, TX, BUF> {
    /// Create a new TX future.
    pub fn new(emac: &'a mut Emac<RX, TX, BUF>, data: &'b [u8]) -> Self {
        Self { emac, data }
    }
}

impl<const RX: usize, const TX: usize, const BUF: usize> Future
    for TxFuture<'_, '_, RX, TX, BUF>
{
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We're not moving anything out of self
        let this = unsafe { self.get_unchecked_mut() };

        // Check for fatal errors
        let status = peek_interrupt_status();
        if status.fatal_bus_error {
            return Poll::Ready(Err(Error::Io(IoError::FrameError)));
        }

        // Check if TX is ready
        if !this.emac.tx_ready() {
            // No TX slot, register waker and wait
            TX_WAKER.register(cx.waker());

            // Double-check after registering
            if !this.emac.tx_ready() {
                return Poll::Pending;
            }
        }

        // TX ready, try to submit
        match this.emac.transmit(this.data) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Future that waits for any error condition.
///
/// Useful for monitoring the EMAC for fatal errors in a background task.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ErrorFuture {
    _phantom: core::marker::PhantomData<()>,
}

impl ErrorFuture {
    /// Create a new error future.
    pub fn new() -> Self {
        Self {
            _phantom: core::marker::PhantomData,
        }
    }
}

impl Default for ErrorFuture {
    fn default() -> Self {
        Self::new()
    }
}

impl Future for ErrorFuture {
    type Output = InterruptStatus;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let status = peek_interrupt_status();
        if status.has_error() {
            Poll::Ready(status)
        } else {
            ERR_WAKER.register(cx.waker());
            Poll::Pending
        }
    }
}

// =============================================================================
// Extension Trait
// =============================================================================

/// Extension trait providing async methods for EMAC.
///
/// This trait is automatically implemented for all `Emac` instances when
/// the `async` feature is enabled.
pub trait AsyncEmacExt {
    /// Receive a frame asynchronously.
    ///
    /// This method yields until a frame is available, using wakers to avoid
    /// busy-waiting.
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
    /// - A fatal bus error occurred
    /// - Frame had CRC/length errors
    /// - Buffer too small for frame
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut buf = [0u8; 1600];
    /// loop {
    ///     let len = emac.receive_async(&mut buf).await?;
    ///     process_frame(&buf[..len]);
    /// }
    /// ```
    fn receive_async<'a, 'b>(
        &'a mut self,
        buffer: &'b mut [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a;

    /// Transmit a frame asynchronously.
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
    /// Returns an error if:
    /// - A fatal bus error occurred
    /// - Frame is too large for TX buffer
    ///
    /// # Example
    ///
    /// ```ignore
    /// let packet = build_packet();
    /// let len = emac.transmit_async(&packet).await?;
    /// ```
    fn transmit_async<'a, 'b>(
        &'a mut self,
        data: &'b [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a;

    /// Wait for any error condition.
    ///
    /// This is useful for a background task that monitors for fatal errors.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In error monitoring task
    /// let status = wait_for_error().await;
    /// log::error!("EMAC error: {:?}", status);
    /// // Attempt recovery...
    /// ```
    fn wait_for_error() -> impl Future<Output = InterruptStatus>;
}

impl<const RX: usize, const TX: usize, const BUF: usize> AsyncEmacExt for Emac<RX, TX, BUF> {
    fn receive_async<'a, 'b>(
        &'a mut self,
        buffer: &'b mut [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a,
    {
        RxFuture::new(self, buffer)
    }

    fn transmit_async<'a, 'b>(
        &'a mut self,
        data: &'b [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a,
    {
        TxFuture::new(self, data)
    }

    fn wait_for_error() -> impl Future<Output = InterruptStatus> {
        ErrorFuture::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Reset all async state.
///
/// Call this when reinitializing the EMAC to clear any stale wakers.
pub fn reset_async_state() {
    // Wake any pending futures so they can complete/error
    RX_WAKER.wake();
    TX_WAKER.wake();
    ERR_WAKER.wake();
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::task::{RawWaker, RawWakerVTable, Waker};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // =========================================================================
    // Test Waker Implementation
    // =========================================================================

    /// Counter for tracking waker calls
    struct WakeCounter {
        count: AtomicUsize,
    }

    impl WakeCounter {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                count: AtomicUsize::new(0),
            })
        }

        fn count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    /// Create a test waker that increments a counter when woken
    fn test_waker(counter: Arc<WakeCounter>) -> Waker {
        fn clone_fn(ptr: *const ()) -> RawWaker {
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            let cloned = arc.clone();
            core::mem::forget(arc); // Don't decrement ref count
            RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
        }

        fn wake_fn(ptr: *const ()) {
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            arc.count.fetch_add(1, Ordering::SeqCst);
            // Don't forget - we're consuming the wake
        }

        fn wake_by_ref_fn(ptr: *const ()) {
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            arc.count.fetch_add(1, Ordering::SeqCst);
            core::mem::forget(arc); // Don't decrement ref count
        }

        fn drop_fn(ptr: *const ()) {
            unsafe {
                Arc::from_raw(ptr as *const WakeCounter);
            }
        }

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(clone_fn, wake_fn, wake_by_ref_fn, drop_fn);

        let raw = RawWaker::new(Arc::into_raw(counter) as *const (), &VTABLE);
        unsafe { Waker::from_raw(raw) }
    }

    // =========================================================================
    // AtomicWaker Tests
    // =========================================================================

    #[test]
    fn atomic_waker_new_is_empty() {
        let waker = AtomicWaker::new();
        assert!(!waker.is_registered());
    }

    #[test]
    fn atomic_waker_default_is_empty() {
        let waker = AtomicWaker::default();
        assert!(!waker.is_registered());
    }

    #[test]
    fn atomic_waker_register_stores_waker() {
        let atomic_waker = AtomicWaker::new();
        let counter = WakeCounter::new();
        let waker = test_waker(counter.clone());

        atomic_waker.register(&waker);
        assert!(atomic_waker.is_registered());
    }

    #[test]
    fn atomic_waker_wake_calls_waker() {
        let atomic_waker = AtomicWaker::new();
        let counter = WakeCounter::new();
        let waker = test_waker(counter.clone());

        atomic_waker.register(&waker);
        assert_eq!(counter.count(), 0);

        atomic_waker.wake();
        assert_eq!(counter.count(), 1);
    }

    #[test]
    fn atomic_waker_wake_clears_waker() {
        let atomic_waker = AtomicWaker::new();
        let counter = WakeCounter::new();
        let waker = test_waker(counter.clone());

        atomic_waker.register(&waker);
        assert!(atomic_waker.is_registered());

        atomic_waker.wake();
        assert!(!atomic_waker.is_registered());
    }

    #[test]
    fn atomic_waker_wake_without_registered_is_noop() {
        let atomic_waker = AtomicWaker::new();
        // Should not panic
        atomic_waker.wake();
        assert!(!atomic_waker.is_registered());
    }

    #[test]
    fn atomic_waker_register_overwrites_previous() {
        let atomic_waker = AtomicWaker::new();
        let counter1 = WakeCounter::new();
        let counter2 = WakeCounter::new();
        let waker1 = test_waker(counter1.clone());
        let waker2 = test_waker(counter2.clone());

        atomic_waker.register(&waker1);
        atomic_waker.register(&waker2);
        atomic_waker.wake();

        // Only the second waker should be called
        assert_eq!(counter1.count(), 0);
        assert_eq!(counter2.count(), 1);
    }

    #[test]
    fn atomic_waker_double_wake_only_wakes_once() {
        let atomic_waker = AtomicWaker::new();
        let counter = WakeCounter::new();
        let waker = test_waker(counter.clone());

        atomic_waker.register(&waker);
        atomic_waker.wake();
        atomic_waker.wake(); // Second wake should be no-op

        assert_eq!(counter.count(), 1);
    }

    // =========================================================================
    // Static Waker Tests
    // =========================================================================

    #[test]
    fn static_wakers_are_independent() {
        // Reset any state from previous tests
        RX_WAKER.wake();
        TX_WAKER.wake();
        ERR_WAKER.wake();

        // Register different wakers
        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();
        let err_counter = WakeCounter::new();

        RX_WAKER.register(&test_waker(rx_counter.clone()));
        TX_WAKER.register(&test_waker(tx_counter.clone()));
        ERR_WAKER.register(&test_waker(err_counter.clone()));

        // Wake only RX
        RX_WAKER.wake();
        assert_eq!(rx_counter.count(), 1);
        assert_eq!(tx_counter.count(), 0);
        assert_eq!(err_counter.count(), 0);

        // Re-register RX and wake TX
        RX_WAKER.register(&test_waker(rx_counter.clone()));
        TX_WAKER.wake();
        assert_eq!(rx_counter.count(), 1); // Not woken again
        assert_eq!(tx_counter.count(), 1);
        assert_eq!(err_counter.count(), 0);
    }

    #[test]
    fn reset_async_state_wakes_all() {
        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();
        let err_counter = WakeCounter::new();

        RX_WAKER.register(&test_waker(rx_counter.clone()));
        TX_WAKER.register(&test_waker(tx_counter.clone()));
        ERR_WAKER.register(&test_waker(err_counter.clone()));

        reset_async_state();

        assert_eq!(rx_counter.count(), 1);
        assert_eq!(tx_counter.count(), 1);
        assert_eq!(err_counter.count(), 1);
    }

    // =========================================================================
    // ErrorFuture Tests
    // =========================================================================

    #[test]
    fn error_future_new() {
        let future = ErrorFuture::new();
        // Should create without panicking
        let _ = future;
    }

    #[test]
    fn error_future_default() {
        let future = ErrorFuture::default();
        let _ = future;
    }
}
