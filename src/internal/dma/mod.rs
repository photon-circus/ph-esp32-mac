//! DMA Engine
//!
//! This module provides the DMA engine for managing TX and RX descriptor rings
//! and buffer transfers. All memory is statically allocated using const generics.
//!
//! # Architecture
//!
//! The DMA engine consists of:
//! - [`DmaEngine`]: Main structure managing RX and TX descriptor rings and buffers
//! - [`DescriptorRing`]: Circular ring buffer for descriptors
//! - Internal descriptor types for RX and TX operations
//!
//! # Note
//!
//! This is an internal module. Many methods are reserved for future use,
//! testing, or debugging purposes.
//!
//! # Example
//!
//! ```ignore
//! use ph_esp32_mac::internal::dma::DmaEngine;
//!
//! // Create DMA engine with 4 RX buffers, 4 TX buffers, 1600 bytes each
//! static mut DMA: DmaEngine<4, 4, 1600> = DmaEngine::new();
//!
//! // Initialize before use
//! unsafe { DMA.init(); }
//! ```

// Allow dead code in this internal module - methods are reserved for future use
#![allow(dead_code)]

mod descriptor;
mod engine;
mod ring;

pub use engine::DmaEngine;
