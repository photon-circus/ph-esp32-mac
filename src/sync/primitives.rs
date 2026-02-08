//! Synchronization primitives for ISR-safe access.
//!
//! Low-level primitives used by the sync and async EMAC wrappers.

use core::cell::RefCell;
#[cfg(any(feature = "async", feature = "embassy-net"))]
use core::task::Waker;
use critical_section::Mutex;

/// Cell providing interior mutability with critical section protection.
///
/// Combines `critical_section::Mutex` with `RefCell` for safe mutable access
/// from both normal code and interrupt handlers.
pub struct CriticalSectionCell<T> {
    inner: Mutex<RefCell<T>>,
}

impl<T> CriticalSectionCell<T> {
    /// Create a new cell (const, suitable for static initialization).
    pub const fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(RefCell::new(value)),
        }
    }

    /// Execute a closure with exclusive mutable access.
    ///
    /// Interrupts are disabled for the duration of the closure.
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

    /// Execute a closure with immutable access.
    #[cfg(any(feature = "async", feature = "embassy-net"))]
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

// SAFETY: CriticalSectionCell uses critical sections to protect all access.
unsafe impl<T> Sync for CriticalSectionCell<T> {}

/// Thread-safe, interrupt-safe waker storage for async I/O.
///
/// Register a waker from async poll, wake from interrupt handler.
#[cfg(any(feature = "async", feature = "embassy-net"))]
pub struct AtomicWaker {
    waker: CriticalSectionCell<Option<Waker>>,
}

#[cfg(any(feature = "async", feature = "embassy-net"))]
impl AtomicWaker {
    /// Create a new empty waker (const, suitable for static initialization).
    pub const fn new() -> Self {
        Self {
            waker: CriticalSectionCell::new(None),
        }
    }

    /// Register a waker to be woken later.
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

    /// Wake the registered waker, if any (clears the stored waker).
    #[inline]
    pub fn wake(&self) {
        let waker = self.waker.with(|slot| slot.take());
        if let Some(w) = waker {
            w.wake();
        }
    }

    /// Check if a waker is currently registered.
    pub fn is_registered(&self) -> bool {
        self.waker.with_ref(|slot| slot.is_some())
    }
}

#[cfg(any(feature = "async", feature = "embassy-net"))]
impl Default for AtomicWaker {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: AtomicWaker uses CriticalSectionCell for synchronization.
#[cfg(any(feature = "async", feature = "embassy-net"))]
unsafe impl Send for AtomicWaker {}
// SAFETY: AtomicWaker uses CriticalSectionCell for synchronization.
#[cfg(any(feature = "async", feature = "embassy-net"))]
unsafe impl Sync for AtomicWaker {}

#[cfg(test)]
#[allow(clippy::std_instead_of_core, clippy::std_instead_of_alloc)]
mod tests {
    extern crate std;

    use super::*;
    #[cfg(any(feature = "async", feature = "embassy-net"))]
    use core::task::{RawWaker, RawWakerVTable};
    #[cfg(any(feature = "async", feature = "embassy-net"))]
    use std::sync::Arc;
    #[cfg(any(feature = "async", feature = "embassy-net"))]
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(any(feature = "async", feature = "embassy-net"))]
    struct WakeCounter {
        count: AtomicUsize,
    }

    #[cfg(any(feature = "async", feature = "embassy-net"))]
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

    #[cfg(any(feature = "async", feature = "embassy-net"))]
    fn test_waker(counter: Arc<WakeCounter>) -> Waker {
        fn clone_fn(ptr: *const ()) -> RawWaker {
            // SAFETY: `ptr` originates from `Arc::into_raw` in this test helper.
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            let cloned = arc.clone();
            core::mem::forget(arc);
            RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
        }

        fn wake_fn(ptr: *const ()) {
            // SAFETY: `ptr` originates from `Arc::into_raw` in this test helper.
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            arc.count.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref_fn(ptr: *const ()) {
            // SAFETY: `ptr` originates from `Arc::into_raw` in this test helper.
            let arc = unsafe { Arc::from_raw(ptr as *const WakeCounter) };
            arc.count.fetch_add(1, Ordering::SeqCst);
            core::mem::forget(arc);
        }

        fn drop_fn(ptr: *const ()) {
            // SAFETY: `ptr` originates from `Arc::into_raw` in this test helper.
            unsafe {
                Arc::from_raw(ptr as *const WakeCounter);
            }
        }

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(clone_fn, wake_fn, wake_by_ref_fn, drop_fn);

        let raw = RawWaker::new(Arc::into_raw(counter) as *const (), &VTABLE);
        // SAFETY: `raw` is built from a valid `RawWakerVTable` and pointer.
        unsafe { Waker::from_raw(raw) }
    }

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

    #[cfg(any(feature = "async", feature = "embassy-net"))]
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

    #[cfg(any(feature = "async", feature = "embassy-net"))]
    #[test]
    fn atomic_waker_new_is_empty() {
        let waker = AtomicWaker::new();
        assert!(!waker.is_registered());
    }

    #[cfg(any(feature = "async", feature = "embassy-net"))]
    #[test]
    fn atomic_waker_default_is_empty() {
        let waker = AtomicWaker::default();
        assert!(!waker.is_registered());
    }

    #[cfg(any(feature = "async", feature = "embassy-net"))]
    #[test]
    fn atomic_waker_register_stores_waker() {
        let atomic_waker = AtomicWaker::new();
        let counter = WakeCounter::new();
        let waker = test_waker(counter.clone());

        atomic_waker.register(&waker);
        assert!(atomic_waker.is_registered());
    }

    #[cfg(any(feature = "async", feature = "embassy-net"))]
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

    #[cfg(any(feature = "async", feature = "embassy-net"))]
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

    #[cfg(any(feature = "async", feature = "embassy-net"))]
    #[test]
    fn atomic_waker_wake_without_registered_is_noop() {
        let atomic_waker = AtomicWaker::new();
        atomic_waker.wake();
        assert!(!atomic_waker.is_registered());
    }

    #[cfg(any(feature = "async", feature = "embassy-net"))]
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

        assert_eq!(counter1.count(), 0);
        assert_eq!(counter2.count(), 1);
    }

    #[cfg(any(feature = "async", feature = "embassy-net"))]
    #[test]
    fn atomic_waker_double_wake_only_wakes_once() {
        let atomic_waker = AtomicWaker::new();
        let counter = WakeCounter::new();
        let waker = test_waker(counter.clone());

        atomic_waker.register(&waker);
        atomic_waker.wake();
        atomic_waker.wake();

        assert_eq!(counter.count(), 1);
    }
}
