//! Synchronization Primitives for ISR-Safe Access
//!
//! This module provides low-level synchronization primitives used by both
//! the synchronous (`sync`) and asynchronous (`asynch`) EMAC modules.
//!
//! # Primitives
//!
//! - [`CriticalSectionCell`] - A wrapper providing interior mutability with
//!   critical section protection, suitable for ISR-safe access.
//! - [`AtomicWaker`] - A thread-safe, interrupt-safe waker storage for async.
//!
//! # Implementation Note
//!
//! These primitives use the `critical-section` crate for synchronization.
//! You must enable the appropriate feature in your HAL crate:
//!
//! ```toml
//! [dependencies]
//! esp-hal = { version = "...", features = ["critical-section"] }
//! ```
//!
//! For ESP32, this typically disables interrupts on the current core.
//! On dual-core configurations, it also acquires a hardware spinlock.

use core::cell::RefCell;
use core::task::Waker;
use critical_section::Mutex;

// =============================================================================
// CriticalSectionCell
// =============================================================================

/// A cell providing interior mutability with critical section protection.
///
/// This wrapper combines `critical_section::Mutex` with `RefCell` to provide
/// safe mutable access to data from both normal code and interrupt handlers.
///
/// # Type Parameters
///
/// * `T` - The type of value to wrap
///
/// # Example
///
/// ```ignore
/// use ph_esp32_mac::sync_primitives::CriticalSectionCell;
///
/// static COUNTER: CriticalSectionCell<u32> = CriticalSectionCell::new(0);
///
/// fn main() {
///     COUNTER.with(|c| *c += 1);
///     let value = COUNTER.with(|c| *c);
///     assert_eq!(value, 1);
/// }
///
/// #[interrupt]
/// fn SOME_IRQ() {
///     // Safe to access from ISR
///     COUNTER.with(|c| *c += 1);
/// }
/// ```
pub struct CriticalSectionCell<T> {
    inner: Mutex<RefCell<T>>,
}

impl<T> CriticalSectionCell<T> {
    /// Create a new critical section cell with the given value.
    ///
    /// This is a const function suitable for static initialization.
    ///
    /// # Arguments
    ///
    /// * `value` - The initial value to wrap
    ///
    /// # Example
    ///
    /// ```ignore
    /// static CELL: CriticalSectionCell<u32> = CriticalSectionCell::new(0);
    /// ```
    pub const fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(RefCell::new(value)),
        }
    }

    /// Execute a closure with exclusive access to the wrapped value.
    ///
    /// This function acquires a critical section (disables interrupts),
    /// then executes the closure with a mutable reference to the value.
    ///
    /// # Arguments
    ///
    /// * `f` - Closure that receives `&mut T` and returns a value
    ///
    /// # Returns
    ///
    /// The return value of the closure.
    ///
    /// # Performance
    ///
    /// Interrupts are disabled for the duration of the closure.
    /// Keep the closure short to minimize interrupt latency.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = CELL.with(|value| {
    ///     *value += 1;
    ///     *value
    /// });
    /// ```
    #[inline]
    pub fn with<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        critical_section::with(|cs| {
            let mut value = self.inner.borrow_ref_mut(cs);
            f(&mut value)
        })
    }

    /// Try to execute a closure, returning `None` if already borrowed.
    ///
    /// This is useful in scenarios where you want to avoid blocking if
    /// another context is currently using the value. Note that since this
    /// uses critical sections, it will still disable interrupts.
    ///
    /// In practice, with proper critical section usage, this should always
    /// succeed (return `Some`), but this method provides a non-panicking
    /// alternative for defensive programming.
    ///
    /// # Returns
    ///
    /// - `Some(result)` if the closure was executed
    /// - `None` if the value was already borrowed (should not happen with proper use)
    #[inline]
    pub fn try_with<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        critical_section::with(|cs| {
            self.inner
                .borrow(cs)
                .try_borrow_mut()
                .ok()
                .map(|mut value| f(&mut value))
        })
    }

    /// Get immutable access to the wrapped value.
    ///
    /// This is useful when you only need to read the value.
    ///
    /// # Arguments
    ///
    /// * `f` - Closure that receives `&T` and returns a value
    ///
    /// # Returns
    ///
    /// The return value of the closure.
    #[inline]
    pub fn with_ref<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        critical_section::with(|cs| {
            let value = self.inner.borrow_ref(cs);
            f(&value)
        })
    }
}

// SAFETY: CriticalSectionCell uses critical sections to protect all access,
// making it safe to share across threads/interrupt contexts.
unsafe impl<T> Sync for CriticalSectionCell<T> {}

// =============================================================================
// AtomicWaker
// =============================================================================

/// A thread-safe, interrupt-safe waker storage.
///
/// `AtomicWaker` allows registering a [`Waker`] that can be later woken from
/// an interrupt handler or another context. This is the core primitive for
/// implementing async I/O with interrupt-driven wakeups.
///
/// # Usage Pattern
///
/// 1. In an async future's `poll()`, register the waker with `register()`
/// 2. In the interrupt handler, call `wake()` when the event occurs
/// 3. The executor will re-poll the future
///
/// # Example
///
/// ```ignore
/// use ph_esp32_mac::sync_primitives::AtomicWaker;
///
/// static RX_WAKER: AtomicWaker = AtomicWaker::new();
///
/// // In async future poll()
/// fn poll(cx: &mut Context) -> Poll<()> {
///     if data_available() {
///         Poll::Ready(())
///     } else {
///         RX_WAKER.register(cx.waker());
///         Poll::Pending
///     }
/// }
///
/// // In ISR
/// #[interrupt]
/// fn RX_IRQ() {
///     RX_WAKER.wake();
/// }
/// ```
pub struct AtomicWaker {
    waker: CriticalSectionCell<Option<Waker>>,
}

impl AtomicWaker {
    /// Create a new empty `AtomicWaker`.
    ///
    /// This is a const function suitable for static initialization.
    ///
    /// # Example
    ///
    /// ```ignore
    /// static WAKER: AtomicWaker = AtomicWaker::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            waker: CriticalSectionCell::new(None),
        }
    }

    /// Register a waker to be woken later.
    ///
    /// Overwrites any previously registered waker. This is an optimization
    /// to avoid cloning the waker if the same task is polling again.
    ///
    /// # Arguments
    ///
    /// * `waker` - The waker to register
    pub fn register(&self, waker: &Waker) {
        self.waker.with(|slot| {
            match slot {
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
    /// This clears the stored waker after waking. If no waker is registered,
    /// this is a no-op.
    #[inline]
    pub fn wake(&self) {
        let waker = self.waker.with(|slot| slot.take());
        if let Some(w) = waker {
            w.wake();
        }
    }

    /// Check if a waker is currently registered.
    ///
    /// # Returns
    ///
    /// `true` if a waker is registered, `false` otherwise.
    pub fn is_registered(&self) -> bool {
        self.waker.with_ref(|slot| slot.is_some())
    }
}

impl Default for AtomicWaker {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: AtomicWaker uses CriticalSectionCell for synchronization
unsafe impl Send for AtomicWaker {}
unsafe impl Sync for AtomicWaker {}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::std_instead_of_core, clippy::std_instead_of_alloc)]
mod tests {
    extern crate std;

    use super::*;
    use core::task::{RawWaker, RawWakerVTable};
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
    // CriticalSectionCell Tests
    // =========================================================================

    #[test]
    fn critical_section_cell_new() {
        let cell: CriticalSectionCell<u32> = CriticalSectionCell::new(42);
        let value = cell.with(|v| *v);
        assert_eq!(value, 42);
    }

    #[test]
    fn critical_section_cell_with_mutates() {
        let cell: CriticalSectionCell<u32> = CriticalSectionCell::new(0);
        cell.with(|v| *v += 10);
        let value = cell.with(|v| *v);
        assert_eq!(value, 10);
    }

    #[test]
    fn critical_section_cell_with_returns_value() {
        let cell: CriticalSectionCell<u32> = CriticalSectionCell::new(42);
        let result = cell.with(|v| *v * 2);
        assert_eq!(result, 84);
    }

    #[test]
    fn critical_section_cell_try_with_succeeds() {
        let cell: CriticalSectionCell<u32> = CriticalSectionCell::new(42);
        let result = cell.try_with(|v| *v);
        assert_eq!(result, Some(42));
    }

    #[test]
    fn critical_section_cell_with_ref_reads() {
        let cell: CriticalSectionCell<u32> = CriticalSectionCell::new(42);
        let value = cell.with_ref(|v| *v);
        assert_eq!(value, 42);
    }

    #[test]
    fn critical_section_cell_static_usage() {
        static CELL: CriticalSectionCell<u32> = CriticalSectionCell::new(0);
        CELL.with(|v| *v = 100);
        let value = CELL.with(|v| *v);
        assert_eq!(value, 100);
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
}
