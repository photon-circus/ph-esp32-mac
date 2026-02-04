//! DMA Descriptor definitions
//!
//! This module defines the TX and RX DMA descriptors used by the EMAC DMA engine
//! for scatter-gather transfers.
//!
//! # Architecture
//!
//! The ESP32 EMAC uses a ring buffer of DMA descriptors for both transmission and
//! reception. Each descriptor points to a data buffer and contains status/control
//! bits that coordinate ownership between the CPU and DMA engine.
//!
//! # Future Improvements
//!
//! ## 1. Typed Error Enums
//!
//! Replace raw error flag accessors with typed enums for better ergonomics:
//!
//! ```ignore
//! #[derive(Debug, Clone, Copy)]
//! #[cfg_attr(feature = "defmt", derive(defmt::Format))]
//! pub enum TxError {
//!     Underflow,
//!     ExcessiveDeferral,
//!     ExcessiveCollision,
//!     LateCollision,
//!     NoCarrier,
//!     LossOfCarrier,
//!     IpPayloadError,
//!     JabberTimeout,
//!     IpHeaderError,
//! }
//!
//! impl TxDescriptor {
//!     pub fn errors(&self) -> impl Iterator<Item = TxError> { ... }
//! }
//! ```
//!
//! ## 2. Builder Pattern for Configuration
//!
//! Add builder-style configuration for complex descriptor setup:
//!
//! ```ignore
//! descriptor
//!     .configure()
//!     .checksum_mode(ChecksumMode::Full)
//!     .enable_timestamp()
//!     .disable_padding()
//!     .apply();
//! ```
//!
//! ## 3. defmt Integration
//!
//! Add `defmt::Format` derives for debugging on embedded targets:
//!
//! ```ignore
//! #[cfg_attr(feature = "defmt", derive(defmt::Format))]
//! pub struct DescriptorStatus {
//!     pub owned: bool,
//!     pub first: bool,
//!     pub last: bool,
//!     pub error: bool,
//!     pub length: usize,
//! }
//! ```
//!
//! ## 4. Const Generic Ring Buffers
//!
//! Type-safe descriptor rings with compile-time size checking:
//!
//! ```ignore
//! pub struct DescriptorRing<D, const N: usize> {
//!     descriptors: [D; N],
//!     head: usize,
//!     tail: usize,
//! }
//! ```

pub mod rx;
pub mod tx;

pub use rx::RxDescriptor;
pub use tx::TxDescriptor;

// Re-export frame size constants from central location for backwards compatibility
pub use crate::internal::constants::{DEFAULT_BUFFER_SIZE, ETH_HEADER_SIZE, MAX_FRAME_SIZE, MTU};

/// Common descriptor ownership bit (bit 31 of first word)
pub const DESC_OWN: u32 = 1 << 31;

/// Descriptor alignment for ESP32 (4 bytes)
#[cfg(not(feature = "esp32p4"))]
pub const DESC_ALIGNMENT: usize = 4;

/// Descriptor alignment for ESP32-P4 (64 bytes for cache line)
#[cfg(feature = "esp32p4")]
pub const DESC_ALIGNMENT: usize = 64;

/// Volatile cell wrapper for descriptor fields
///
/// Ensures all accesses are volatile to prevent compiler optimization
/// from reordering or caching descriptor field accesses.
#[repr(transparent)]
pub struct VolatileCell<T: Copy> {
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
