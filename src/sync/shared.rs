//! ISR-safe EMAC wrappers using critical sections.
//!
//! Provides [`SharedEmac`] for synchronous ISR-safe access and
//! [`AsyncSharedEmac`] for async-capable ISR-safe access.

use super::primitives::CriticalSectionCell;
use crate::driver::mac::Emac;

/// ISR-safe EMAC wrapper using critical sections.
///
/// All access goes through `critical_section::with()`, disabling interrupts
/// for the duration of the closure.
///
/// # Example
///
/// ```ignore
/// static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();
///
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
    /// Create a new shared EMAC instance (const, suitable for static initialization).
    pub const fn new() -> Self {
        Self {
            inner: CriticalSectionCell::new(Emac::new()),
        }
    }

    /// Execute a closure with exclusive access to the EMAC.
    ///
    /// Interrupts are disabled for the duration of the closure.
    #[inline]
    pub fn with<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Emac<RX_BUFS, TX_BUFS, BUF_SIZE>) -> R,
    {
        self.inner.with(f)
    }

    /// Try to execute a closure, returning `None` if already borrowed.
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

/// ISR-safe async-capable EMAC wrapper.
///
/// Combines ISR-safety of [`SharedEmac`] with async/await capabilities.
///
/// # Example
///
/// ```ignore
/// static EMAC: AsyncSharedEmac<10, 10, 1600> = AsyncSharedEmac::new();
///
/// async fn task() {
///     let mut buf = [0u8; 1600];
///     let len = EMAC.receive_async(&mut buf).await.unwrap();
/// }
/// ```
pub struct AsyncSharedEmac<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    inner: CriticalSectionCell<Emac<RX_BUFS, TX_BUFS, BUF_SIZE>>,
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    AsyncSharedEmac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    /// Create a new async shared EMAC instance (const, suitable for static initialization).
    pub const fn new() -> Self {
        Self {
            inner: CriticalSectionCell::new(Emac::new()),
        }
    }

    /// Execute a closure with exclusive access to the EMAC (synchronous).
    #[inline]
    pub fn with<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Emac<RX_BUFS, TX_BUFS, BUF_SIZE>) -> R,
    {
        self.inner.with(f)
    }

    /// Try to execute a closure, returning `None` if already borrowed.
    #[inline]
    pub fn try_with<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Emac<RX_BUFS, TX_BUFS, BUF_SIZE>) -> R,
    {
        self.inner.try_with(f)
    }

    /// Receive a frame asynchronously.
    ///
    /// Yields until a frame is available.
    #[cfg(feature = "async")]
    pub async fn receive_async(&self, buffer: &mut [u8]) -> crate::Result<usize> {
        use super::asynch::RX_WAKER;
        use core::future::poll_fn;
        use core::task::Poll;

        poll_fn(|cx| {
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
                    RX_WAKER.register(cx.waker());
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// Transmit a frame asynchronously.
    ///
    /// Yields until a TX buffer is available.
    #[cfg(feature = "async")]
    pub async fn transmit_async(&self, data: &[u8]) -> crate::Result<usize> {
        use super::asynch::TX_WAKER;
        use core::future::poll_fn;
        use core::task::Poll;

        poll_fn(|cx| {
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
                    TX_WAKER.register(cx.waker());
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// Check if the EMAC has received frames waiting.
    pub fn rx_available(&self) -> bool {
        self.inner.with(|emac| emac.rx_available())
    }

    /// Check if the EMAC can accept a frame for transmission.
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

/// Default shared EMAC configuration (10 RX, 10 TX, 1600 byte buffers).
pub type SharedEmacDefault = SharedEmac<10, 10, 1600>;

/// Small shared EMAC configuration for memory-constrained systems.
pub type SharedEmacSmall = SharedEmac<4, 4, 1600>;

/// Large shared EMAC configuration for high-throughput applications.
pub type SharedEmacLarge = SharedEmac<16, 16, 1600>;

/// Default async shared EMAC configuration.
pub type AsyncSharedEmacDefault = AsyncSharedEmac<10, 10, 1600>;

/// Small async shared EMAC configuration.
pub type AsyncSharedEmacSmall = AsyncSharedEmac<4, 4, 1600>;

/// Large async shared EMAC configuration.
pub type AsyncSharedEmacLarge = AsyncSharedEmac<16, 16, 1600>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::config::State;

    #[test]
    fn test_shared_emac_new() {
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

    #[test]
    fn test_static_shared_emac() {
        static SHARED: SharedEmac<10, 10, 1600> = SharedEmac::new();

        let state = SHARED.with(|emac| emac.state());
        assert_eq!(state, State::Uninitialized);
    }

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

    #[test]
    fn test_async_shared_emac_rx_available_uninitialized() {
        let shared: AsyncSharedEmacDefault = AsyncSharedEmac::new();
        assert!(!shared.rx_available());
    }

    #[test]
    fn test_async_shared_emac_tx_ready_uninitialized() {
        let shared: AsyncSharedEmacDefault = AsyncSharedEmac::new();
        assert!(shared.tx_ready());
    }

    #[test]
    fn test_static_async_shared_emac() {
        static SHARED: AsyncSharedEmac<10, 10, 1600> = AsyncSharedEmac::new();

        let state = SHARED.with(|emac| emac.state());
        assert_eq!(state, State::Uninitialized);
    }
}
