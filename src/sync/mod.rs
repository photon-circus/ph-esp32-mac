//! Synchronization and Concurrency Support
//!
//! This module provides synchronization primitives and concurrency-safe wrappers
//! for the EMAC driver. It includes:
//!
//! - **Primitives** (`primitives`): Low-level synchronization types
//!   - [`CriticalSectionCell`] - ISR-safe interior mutability
//!   - [`AtomicWaker`] - Async waker storage for interrupts
//!
//! - **Shared Wrappers** (`shared`): ISR-safe EMAC wrappers
//!   - [`SharedEmac`] - Synchronous critical-section protected EMAC
//!   - [`AsyncSharedEmac`] - Async-capable ISR-safe EMAC wrapper
//!
//! - **Async Support** (`asynch`): Async/await support for EMAC operations
//!   - [`AsyncEmacExt`] - Extension trait adding async methods to EMAC
//!   - [`RxFuture`], [`TxFuture`] - Futures for async I/O
//!   - Static wakers and interrupt handler
//!
//! # Feature Flags
//!
//! - `critical-section`: Enables `primitives` and `shared` modules
//! - `async`: Enables `asynch` module (also requires `critical-section`)
//!
//! # Example
//!
//! ```ignore
//! use ph_esp32_mac::sync::{SharedEmac, AtomicWaker, CriticalSectionCell};
//!
//! // Static ISR-safe EMAC
//! static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();
//!
//! fn main() {
//!     EMAC.with(|emac| {
//!         emac.init(EmacConfig::default()).unwrap();
//!         emac.start().unwrap();
//!     });
//! }
//!
//! #[interrupt]
//! fn EMAC_IRQ() {
//!     EMAC.with(|emac| {
//!         // Safe access from ISR
//!         let status = emac.read_interrupt_status();
//!         emac.clear_interrupts(status);
//!     });
//! }
//! ```

// Primitives module (requires critical-section)
mod primitives;

pub use primitives::{AtomicWaker, CriticalSectionCell};

// Shared wrappers (requires critical-section)
mod shared;

pub use shared::{
    AsyncSharedEmac, AsyncSharedEmacDefault, AsyncSharedEmacLarge, AsyncSharedEmacSmall,
    SharedEmac, SharedEmacDefault, SharedEmacLarge, SharedEmacSmall,
};

// Async support (requires async feature)
#[cfg(feature = "async")]
pub mod asynch;

#[cfg(feature = "async")]
pub use asynch::{
    async_interrupt_handler, peek_interrupt_status, reset_async_state, AsyncEmacExt, ErrorFuture,
    RxFuture, TxFuture, ERR_WAKER, RX_WAKER, TX_WAKER,
};
