//! DMA Engine
//!
//! Manages TX and RX descriptor rings and buffer transfers for the EMAC.
//! All memory is statically allocated using const generics.

// Allow dead code - methods reserved for future async/interrupt-driven use
#![allow(dead_code)]

mod descriptor;
mod engine;
mod ring;

pub use engine::DmaEngine;
