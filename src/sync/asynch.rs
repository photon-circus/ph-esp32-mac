//! Async/await support for EMAC operations.
//!
//! Provides futures, wakers, and an interrupt handler for async I/O.

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use super::primitives::AtomicWaker;
use crate::{Emac, Error, InterruptStatus, IoError, Result};

/// Waker for RX complete events.
pub static RX_WAKER: AtomicWaker = AtomicWaker::new();

/// Waker for TX complete events.
pub static TX_WAKER: AtomicWaker = AtomicWaker::new();

/// Waker for error events (underflow, overflow, bus error).
pub static ERR_WAKER: AtomicWaker = AtomicWaker::new();

/// Async-aware interrupt handler for EMAC.
///
/// Call from your EMAC interrupt handler when using async operations.
///
/// # Example
///
/// ```ignore
/// #[interrupt]
/// fn ETH_MAC() {
///     ph_esp32_mac::sync::asynch::async_interrupt_handler();
/// }
/// ```
#[inline]
pub fn async_interrupt_handler() {
    let status = InterruptStatus::from_raw(crate::internal::register::dma::DmaRegs::status());

    if status.rx_complete || status.rx_buf_unavailable {
        RX_WAKER.wake();
    }

    if status.tx_complete || status.tx_buf_unavailable {
        TX_WAKER.wake();
    }

    if status.has_error() {
        ERR_WAKER.wake();
        RX_WAKER.wake();
        TX_WAKER.wake();
    }

    crate::internal::register::dma::DmaRegs::set_status(status.to_raw());
}

/// Returns the last interrupt status without clearing.
#[inline]
pub fn peek_interrupt_status() -> InterruptStatus {
    InterruptStatus::from_raw(crate::internal::register::dma::DmaRegs::status())
}

/// Future for async receive operations.
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

impl<const RX: usize, const TX: usize, const BUF: usize> Future for RxFuture<'_, '_, RX, TX, BUF> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        let status = peek_interrupt_status();
        if status.fatal_bus_error {
            return Poll::Ready(Err(Error::Io(IoError::FrameError)));
        }

        if !this.emac.rx_available() {
            RX_WAKER.register(cx.waker());
            if !this.emac.rx_available() {
                return Poll::Pending;
            }
        }

        match this.emac.receive(this.buffer) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Future for async transmit operations.
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

impl<const RX: usize, const TX: usize, const BUF: usize> Future for TxFuture<'_, '_, RX, TX, BUF> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        let status = peek_interrupt_status();
        if status.fatal_bus_error {
            return Poll::Ready(Err(Error::Io(IoError::FrameError)));
        }

        if !this.emac.tx_ready() {
            TX_WAKER.register(cx.waker());
            if !this.emac.tx_ready() {
                return Poll::Pending;
            }
        }

        match this.emac.transmit(this.data) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Future that waits for any error condition.
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

/// Extension trait providing async methods for EMAC.
pub trait AsyncEmacExt {
    /// Receive a frame asynchronously.
    fn receive_async<'a, 'b>(
        &'a mut self,
        buffer: &'b mut [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a;

    /// Transmit a frame asynchronously.
    fn transmit_async<'a, 'b>(
        &'a mut self,
        data: &'b [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a;

    /// Wait for any error condition.
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

/// Reset all async state (call when reinitializing EMAC).
pub fn reset_async_state() {
    RX_WAKER.wake();
    TX_WAKER.wake();
    ERR_WAKER.wake();
}

#[cfg(test)]
#[allow(clippy::std_instead_of_core, clippy::std_instead_of_alloc)]
mod tests {
    extern crate std;

    use super::*;
    use core::task::{RawWaker, RawWakerVTable, Waker};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

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

    fn test_waker(counter: Arc<WakeCounter>) -> Waker {
        fn clone_fn(ptr: *const ()) -> RawWaker {
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            let cloned = arc.clone();
            core::mem::forget(arc);
            RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
        }

        fn wake_fn(ptr: *const ()) {
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            arc.count.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref_fn(ptr: *const ()) {
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            arc.count.fetch_add(1, Ordering::SeqCst);
            core::mem::forget(arc);
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

    #[test]
    fn static_wakers_are_independent() {
        RX_WAKER.wake();
        TX_WAKER.wake();
        ERR_WAKER.wake();

        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();
        let err_counter = WakeCounter::new();

        RX_WAKER.register(&test_waker(rx_counter.clone()));
        TX_WAKER.register(&test_waker(tx_counter.clone()));
        ERR_WAKER.register(&test_waker(err_counter.clone()));

        RX_WAKER.wake();
        assert_eq!(rx_counter.count(), 1);
        assert_eq!(tx_counter.count(), 0);
        assert_eq!(err_counter.count(), 0);

        RX_WAKER.register(&test_waker(rx_counter.clone()));
        TX_WAKER.wake();
        assert_eq!(rx_counter.count(), 1);
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

    #[test]
    fn error_future_new() {
        let future = ErrorFuture::new();
        let _ = future;
    }

    #[test]
    fn error_future_default() {
        let future = ErrorFuture::default();
        let _ = future;
    }
}
