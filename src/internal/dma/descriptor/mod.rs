//! TX and RX DMA descriptor structures.
//!
//! Each descriptor points to a data buffer and contains status/control bits
//! for CPU/DMA ownership coordination.

pub mod bits;
pub mod rx;
pub mod tx;

pub use rx::RxDescriptor;
pub use tx::TxDescriptor;

/// Volatile cell wrapper for descriptor fields
///
/// Ensures all accesses are volatile to prevent compiler optimization
/// from reordering or caching descriptor field accesses.
#[repr(transparent)]
pub(crate) struct VolatileCell<T: Copy> {
    value: core::cell::UnsafeCell<T>,
}

// Safety: VolatileCell is safe to share between threads because all access
// is through volatile operations which are atomic for u32 on ESP32.
unsafe impl<T: Copy> Sync for VolatileCell<T> {}

impl<T: Copy> VolatileCell<T> {
    /// Create a new volatile cell with the given initial value
    #[inline(always)]
    pub const fn new(value: T) -> Self {
        Self {
            value: core::cell::UnsafeCell::new(value),
        }
    }

    /// Read the value (volatile read)
    #[inline(always)]
    pub fn get(&self) -> T {
        unsafe { core::ptr::read_volatile(self.value.get()) }
    }

    /// Write a value (volatile write)
    #[inline(always)]
    pub fn set(&self, value: T) {
        unsafe { core::ptr::write_volatile(self.value.get(), value) }
    }

    /// Update the value using a function (read-modify-write)
    #[inline(always)]
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(T) -> T,
    {
        let old = self.get();
        self.set(f(old));
    }
}

impl<T: Copy + Default> Default for VolatileCell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
