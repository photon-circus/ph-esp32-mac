//! Async/await support for EMAC operations.
//!
//! Provides futures, per-instance wakers, and an interrupt handler for async I/O.

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use super::primitives::AtomicWaker;
use crate::internal::register::dma::DmaRegs;
use crate::{Emac, Error, InterruptStatus, IoError, Result};

/// Per-instance async state for EMAC wakers.
///
/// Store this in static memory and pass a reference to async operations
/// and interrupt handlers.
pub struct AsyncEmacState {
    rx_waker: AtomicWaker,
    tx_waker: AtomicWaker,
    err_waker: AtomicWaker,
}

impl AsyncEmacState {
    /// Create a new async state.
    pub const fn new() -> Self {
        Self {
            rx_waker: AtomicWaker::new(),
            tx_waker: AtomicWaker::new(),
            err_waker: AtomicWaker::new(),
        }
    }

    /// Register a waker for RX completion events.
    ///
    /// # Arguments
    ///
    /// * `waker` - Task waker to register
    pub(crate) fn register_rx(&self, waker: &Waker) {
        self.rx_waker.register(waker);
    }

    /// Register a waker for TX completion events.
    ///
    /// # Arguments
    ///
    /// * `waker` - Task waker to register
    pub(crate) fn register_tx(&self, waker: &Waker) {
        self.tx_waker.register(waker);
    }

    /// Register a waker for error events.
    ///
    /// # Arguments
    ///
    /// * `waker` - Task waker to register
    pub(crate) fn register_err(&self, waker: &Waker) {
        self.err_waker.register(waker);
    }

    /// Wake all registered wakers (call when reinitializing EMAC).
    pub fn reset(&self) {
        self.rx_waker.wake();
        self.tx_waker.wake();
        self.err_waker.wake();
    }

    /// Wake RX/TX/error tasks based on an interrupt status snapshot.
    ///
    /// # Arguments
    ///
    /// * `status` - Interrupt status snapshot to interpret
    pub fn on_interrupt(&self, status: InterruptStatus) {
        if status.rx_complete || status.rx_buf_unavailable {
            self.rx_waker.wake();
        }

        if status.tx_complete || status.tx_buf_unavailable {
            self.tx_waker.wake();
        }

        if status.has_error() {
            self.err_waker.wake();
            self.rx_waker.wake();
            self.tx_waker.wake();
        }
    }

    /// Handle the EMAC interrupt and wake async tasks.
    ///
    /// This reads the DMA interrupt status, wakes any waiting tasks, and
    /// clears the interrupt flags.
    pub fn handle_interrupt(&self) {
        let status = InterruptStatus::from_raw(DmaRegs::status());
        self.on_interrupt(status);
        DmaRegs::set_status(status.to_raw());
    }
}

impl Default for AsyncEmacState {
    fn default() -> Self {
        Self::new()
    }
}

/// Async-aware interrupt handler for EMAC.
///
/// Call from your EMAC interrupt handler when using async operations.
///
/// # Arguments
///
/// * `state` - Async waker state associated with this EMAC instance
///
/// # Example
///
/// ```ignore
/// static ASYNC_STATE: AsyncEmacState = AsyncEmacState::new();
///
/// #[interrupt]
/// fn ETH_MAC() {
///     ph_esp32_mac::sync::asynch::async_interrupt_handler(&ASYNC_STATE);
/// }
/// ```
#[inline]
pub fn async_interrupt_handler(state: &AsyncEmacState) {
    state.handle_interrupt();
}

/// Returns the last interrupt status without clearing.
#[inline]
pub fn peek_interrupt_status() -> InterruptStatus {
    InterruptStatus::from_raw(crate::internal::register::dma::DmaRegs::status())
}

/// Reset all async state (call when reinitializing EMAC).
///
/// # Arguments
///
/// * `state` - Async waker state associated with this EMAC instance
#[inline]
pub fn reset_async_state(state: &AsyncEmacState) {
    state.reset();
}

/// Future for async receive operations.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct RxFuture<'a, 'b, const RX: usize, const TX: usize, const BUF: usize> {
    emac: &'a mut Emac<RX, TX, BUF>,
    state: &'a AsyncEmacState,
    buffer: &'b mut [u8],
}

impl<'a, 'b, const RX: usize, const TX: usize, const BUF: usize> RxFuture<'a, 'b, RX, TX, BUF> {
    /// Create a new RX future.
    ///
    /// # Arguments
    ///
    /// * `emac` - EMAC instance to receive from
    /// * `state` - Async waker state for this EMAC instance
    /// * `buffer` - Destination buffer for the received frame
    pub fn new(
        emac: &'a mut Emac<RX, TX, BUF>,
        state: &'a AsyncEmacState,
        buffer: &'b mut [u8],
    ) -> Self {
        Self {
            emac,
            state,
            buffer,
        }
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
            this.state.register_rx(cx.waker());
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
    state: &'a AsyncEmacState,
    data: &'b [u8],
}

impl<'a, 'b, const RX: usize, const TX: usize, const BUF: usize> TxFuture<'a, 'b, RX, TX, BUF> {
    /// Create a new TX future.
    ///
    /// # Arguments
    ///
    /// * `emac` - EMAC instance to transmit from
    /// * `state` - Async waker state for this EMAC instance
    /// * `data` - Frame data to transmit
    pub fn new(emac: &'a mut Emac<RX, TX, BUF>, state: &'a AsyncEmacState, data: &'b [u8]) -> Self {
        Self { emac, state, data }
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
            this.state.register_tx(cx.waker());
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
pub struct ErrorFuture<'a> {
    state: &'a AsyncEmacState,
}

impl<'a> ErrorFuture<'a> {
    /// Create a new error future.
    ///
    /// # Arguments
    ///
    /// * `state` - Async waker state for this EMAC instance
    pub fn new(state: &'a AsyncEmacState) -> Self {
        Self { state }
    }
}

impl Future for ErrorFuture<'_> {
    type Output = InterruptStatus;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let status = peek_interrupt_status();
        if status.has_error() {
            Poll::Ready(status)
        } else {
            self.state.register_err(cx.waker());
            Poll::Pending
        }
    }
}

/// Extension trait providing async methods for EMAC.
pub trait AsyncEmacExt {
    /// Receive a frame asynchronously.
    ///
    /// # Arguments
    ///
    /// * `state` - Async waker state for this EMAC instance
    /// * `buffer` - Destination buffer for the received frame
    ///
    /// # Errors
    ///
    /// Propagates errors from [`Emac::receive`].
    fn receive_async<'a, 'b>(
        &'a mut self,
        state: &'a AsyncEmacState,
        buffer: &'b mut [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a;

    /// Transmit a frame asynchronously.
    ///
    /// # Arguments
    ///
    /// * `state` - Async waker state for this EMAC instance
    /// * `data` - Frame data to transmit
    ///
    /// # Errors
    ///
    /// Propagates errors from [`Emac::transmit`].
    fn transmit_async<'a, 'b>(
        &'a mut self,
        state: &'a AsyncEmacState,
        data: &'b [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a;

    /// Wait for any error condition.
    ///
    /// # Arguments
    ///
    /// * `state` - Async waker state for this EMAC instance
    fn wait_for_error<'a>(
        &'a self,
        state: &'a AsyncEmacState,
    ) -> impl Future<Output = InterruptStatus> + 'a;
}

impl<const RX: usize, const TX: usize, const BUF: usize> AsyncEmacExt for Emac<RX, TX, BUF> {
    fn receive_async<'a, 'b>(
        &'a mut self,
        state: &'a AsyncEmacState,
        buffer: &'b mut [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a,
    {
        RxFuture::new(self, state, buffer)
    }

    fn transmit_async<'a, 'b>(
        &'a mut self,
        state: &'a AsyncEmacState,
        data: &'b [u8],
    ) -> impl Future<Output = Result<usize>> + 'a
    where
        'b: 'a,
    {
        TxFuture::new(self, state, data)
    }

    fn wait_for_error<'a>(
        &'a self,
        state: &'a AsyncEmacState,
    ) -> impl Future<Output = InterruptStatus> + 'a {
        let _ = self;
        ErrorFuture::new(state)
    }
}

#[cfg(test)]
#[allow(clippy::std_instead_of_core, clippy::std_instead_of_alloc)]
mod tests {
    extern crate std;

    use super::*;
    use core::task::{RawWaker, RawWakerVTable, Waker};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn async_states_are_independent() {
        let state_a = AsyncEmacState::new();
        let state_b = AsyncEmacState::new();

        let rx_counter_a = WakeCounter::new();
        let rx_counter_b = WakeCounter::new();
        let tx_counter_a = WakeCounter::new();

        state_a.register_rx(&test_waker(rx_counter_a.clone()));
        state_b.register_rx(&test_waker(rx_counter_b.clone()));
        state_a.register_tx(&test_waker(tx_counter_a.clone()));

        let mut status = InterruptStatus::default();
        status.rx_complete = true;
        state_a.on_interrupt(status);

        assert_eq!(rx_counter_a.count(), 1);
        assert_eq!(rx_counter_b.count(), 0);
        assert_eq!(tx_counter_a.count(), 0);
    }

    #[test]
    fn reset_async_state_wakes_all() {
        let state = AsyncEmacState::new();
        let rx_counter = WakeCounter::new();
        let tx_counter = WakeCounter::new();
        let err_counter = WakeCounter::new();

        state.register_rx(&test_waker(rx_counter.clone()));
        state.register_tx(&test_waker(tx_counter.clone()));
        state.register_err(&test_waker(err_counter.clone()));

        state.reset();

        assert_eq!(rx_counter.count(), 1);
        assert_eq!(tx_counter.count(), 1);
        assert_eq!(err_counter.count(), 1);
    }

    #[test]
    fn error_future_new() {
        let state = AsyncEmacState::new();
        let future = ErrorFuture::new(&state);
        let _ = future;
    }
}
