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
//! # Example
//!
//! ```ignore
//! use ph_esp32_mac::dma::DmaEngine;
//!
//! // Create DMA engine with 4 RX buffers, 4 TX buffers, 1600 bytes each
//! static mut DMA: DmaEngine<4, 4, 1600> = DmaEngine::new();
//!
//! // Initialize before use
//! unsafe { DMA.init(); }
//! ```

// Internal descriptor module
mod descriptor;

use descriptor::{RxDescriptor, TxDescriptor};
use crate::driver::error::{DmaError, IoError, Result};
use crate::internal::register::dma::DmaRegs;

// =============================================================================
// Descriptor Ring
// =============================================================================

/// Circular descriptor ring for DMA operations
///
/// Manages a fixed-size ring of descriptors with a current index pointer.
/// The ring wraps around automatically.
pub struct DescriptorRing<D, const N: usize> {
    /// Array of descriptors
    descriptors: [D; N],
    /// Current index for processing
    current: usize,
}

impl<D, const N: usize> DescriptorRing<D, N> {
    /// Create a new descriptor ring from an existing array
    #[must_use]
    pub const fn from_array(descriptors: [D; N]) -> Self {
        Self {
            descriptors,
            current: 0,
        }
    }

    /// Get the number of descriptors in the ring
    #[inline(always)]
    #[must_use]
    pub const fn len(&self) -> usize {
        N
    }

    /// Check if the ring is empty (always false for fixed-size ring)
    #[inline(always)]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        N == 0
    }

    /// Get the current index
    #[inline(always)]
    #[must_use]
    pub const fn current_index(&self) -> usize {
        self.current
    }

    /// Advance the current index by one, wrapping around
    #[inline(always)]
    pub fn advance(&mut self) {
        self.current = (self.current + 1) % N;
    }

    /// Advance the current index by n, wrapping around
    #[inline(always)]
    pub fn advance_by(&mut self, n: usize) {
        self.current = (self.current + n) % N;
    }

    /// Reset the current index to 0
    #[inline(always)]
    pub fn reset(&mut self) {
        self.current = 0;
    }

    /// Get a reference to the current descriptor
    #[inline(always)]
    pub fn current(&self) -> &D {
        &self.descriptors[self.current]
    }

    /// Get a mutable reference to the current descriptor
    #[inline(always)]
    pub fn current_mut(&mut self) -> &mut D {
        &mut self.descriptors[self.current]
    }

    /// Get a reference to a descriptor at a specific index
    #[inline(always)]
    pub fn get(&self, index: usize) -> &D {
        &self.descriptors[index % N]
    }

    /// Get a mutable reference to a descriptor at a specific index
    #[inline(always)]
    pub fn get_mut(&mut self, index: usize) -> &mut D {
        &mut self.descriptors[index % N]
    }

    /// Get a reference to a descriptor at an offset from current
    #[inline(always)]
    pub fn at_offset(&self, offset: usize) -> &D {
        &self.descriptors[(self.current + offset) % N]
    }

    /// Get the base address of the descriptor array
    #[inline(always)]
    pub fn base_addr(&self) -> *const D {
        self.descriptors.as_ptr()
    }

    /// Get the base address as u32 (for DMA register)
    #[inline(always)]
    pub fn base_addr_u32(&self) -> u32 {
        self.descriptors.as_ptr() as u32
    }

    /// Iterate over all descriptors
    pub fn iter(&self) -> impl Iterator<Item = &D> {
        self.descriptors.iter()
    }

    /// Iterate mutably over all descriptors
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut D> {
        self.descriptors.iter_mut()
    }
}

// =============================================================================
// DMA Engine
// =============================================================================

/// DMA Engine with statically allocated buffers
///
/// This structure manages TX and RX descriptor rings and their associated
/// data buffers. All memory is allocated at compile time using const generics.
///
/// # Type Parameters
/// * `RX_BUFS` - Number of receive buffers/descriptors
/// * `TX_BUFS` - Number of transmit buffers/descriptors
/// * `BUF_SIZE` - Size of each buffer in bytes (should be >= 1600 for jumbo frames)
///
/// # Example
/// ```ignore
/// // Create a DMA engine with 10 RX buffers, 10 TX buffers, 1600 bytes each
/// static mut DMA: DmaEngine<10, 10, 1600> = DmaEngine::new();
/// ```
pub struct DmaEngine<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    /// RX descriptor ring
    rx_ring: DescriptorRing<RxDescriptor, RX_BUFS>,
    /// TX descriptor ring
    tx_ring: DescriptorRing<TxDescriptor, TX_BUFS>,
    /// RX data buffers
    rx_buffers: [[u8; BUF_SIZE]; RX_BUFS],
    /// TX data buffers
    tx_buffers: [[u8; BUF_SIZE]; TX_BUFS],
    /// TX control flags to apply to frames
    tx_ctrl_flags: u32,
    /// Whether the engine has been initialized
    initialized: bool,
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    DmaEngine<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    /// Create a new DMA engine with zeroed buffers
    ///
    /// This is a const function and can be used to initialize static variables.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rx_ring: DescriptorRing {
                descriptors: [const { RxDescriptor::new() }; RX_BUFS],
                current: 0,
            },
            tx_ring: DescriptorRing {
                descriptors: [const { TxDescriptor::new() }; TX_BUFS],
                current: 0,
            },
            rx_buffers: [[0u8; BUF_SIZE]; RX_BUFS],
            tx_buffers: [[0u8; BUF_SIZE]; TX_BUFS],
            tx_ctrl_flags: 0,
            initialized: false,
        }
    }

    /// Get the total memory usage of this DMA engine in bytes
    #[must_use]
    pub const fn memory_usage() -> usize {
        // Descriptors
        let rx_desc_size = RX_BUFS * RxDescriptor::SIZE;
        let tx_desc_size = TX_BUFS * TxDescriptor::SIZE;
        // Buffers
        let rx_buf_size = RX_BUFS * BUF_SIZE;
        let tx_buf_size = TX_BUFS * BUF_SIZE;
        // Total
        rx_desc_size + tx_desc_size + rx_buf_size + tx_buf_size
    }

    /// Initialize the DMA descriptor chains
    ///
    /// This must be called before any DMA operations. It sets up the
    /// descriptor chains in circular mode and configures the DMA registers.
    ///
    /// # Safety
    /// This function accesses hardware registers. The caller must ensure:
    /// - The EMAC peripheral clock is enabled
    /// - No DMA operations are in progress
    /// - This is only called once, or after a full reset
    pub fn init(&mut self) {
        // Initialize RX descriptor chain
        for i in 0..RX_BUFS {
            let next_idx = (i + 1) % RX_BUFS;
            let buffer_ptr = self.rx_buffers[i].as_mut_ptr();
            let next_desc = &self.rx_ring.descriptors[next_idx] as *const RxDescriptor;

            self.rx_ring.descriptors[i].setup_chained(buffer_ptr, BUF_SIZE, next_desc);
        }

        // Initialize TX descriptor chain
        for i in 0..TX_BUFS {
            let next_idx = (i + 1) % TX_BUFS;
            let buffer_ptr = self.tx_buffers[i].as_ptr();
            let next_desc = &self.tx_ring.descriptors[next_idx] as *const TxDescriptor;

            self.tx_ring.descriptors[i].setup_chained(buffer_ptr, next_desc);
        }

        // Reset indices
        self.rx_ring.reset();
        self.tx_ring.reset();

        // Set descriptor base addresses in DMA registers
        DmaRegs::set_rx_desc_list_addr(self.rx_ring.base_addr_u32());
        DmaRegs::set_tx_desc_list_addr(self.tx_ring.base_addr_u32());

        self.initialized = true;
    }

    /// Reset the DMA engine to initial state
    ///
    /// Re-initializes all descriptors and resets indices. Does not
    /// touch the DMA registers (caller should stop DMA first).
    pub fn reset(&mut self) {
        // Reset RX descriptors - give all back to DMA
        for i in 0..RX_BUFS {
            self.rx_ring.descriptors[i].recycle();
        }

        // Reset TX descriptors - CPU owns all
        for i in 0..TX_BUFS {
            self.tx_ring.descriptors[i].reset();
        }

        // Reset indices
        self.rx_ring.reset();
        self.tx_ring.reset();

        // Update DMA registers with base addresses
        DmaRegs::set_rx_desc_list_addr(self.rx_ring.base_addr_u32());
        DmaRegs::set_tx_desc_list_addr(self.tx_ring.base_addr_u32());
    }

    /// Check if the DMA engine has been initialized
    #[inline(always)]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    // =========================================================================
    // TX Operations
    // =========================================================================

    /// Set TX control flags to apply to all transmitted frames
    ///
    /// These flags are OR'd with the descriptor flags for each frame.
    /// Useful for enabling checksum offload, timestamping, etc.
    pub fn set_tx_ctrl_flags(&mut self, flags: u32) {
        self.tx_ctrl_flags = flags;
    }

    /// Get the current TX control flags
    #[inline(always)]
    pub fn tx_ctrl_flags(&self) -> u32 {
        self.tx_ctrl_flags
    }

    /// Check how many TX descriptors are available (not owned by DMA)
    pub fn tx_available(&self) -> usize {
        let mut count = 0;
        for i in 0..TX_BUFS {
            let idx = (self.tx_ring.current + i) % TX_BUFS;
            if !self.tx_ring.descriptors[idx].is_owned() {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Check if there are enough descriptors for a frame of given size
    pub fn can_transmit(&self, len: usize) -> bool {
        if len == 0 || len > BUF_SIZE * TX_BUFS {
            return false;
        }
        let needed = len.div_ceil(BUF_SIZE);
        self.tx_available() >= needed
    }

    /// Transmit a frame
    ///
    /// Copies the frame data to TX buffers and submits to DMA for transmission.
    /// Supports frames larger than a single buffer (scatter-gather).
    ///
    /// # Arguments
    /// * `data` - Frame data to transmit (must be non-empty)
    ///
    /// # Returns
    /// * `Ok(len)` - Number of bytes submitted for transmission
    /// * `Err(Error::Dma(DmaError::InvalidLength))` - Data is empty
    /// * `Err(Error::Dma(DmaError::FrameTooLarge))` - Data exceeds total buffer capacity
    /// * `Err(Error::Dma(DmaError::NoDescriptorsAvailable))` - Not enough free descriptors
    pub fn transmit(&mut self, data: &[u8]) -> Result<usize> {
        if data.is_empty() {
            return Err(DmaError::InvalidLength.into());
        }

        let total_capacity = BUF_SIZE * TX_BUFS;
        if data.len() > total_capacity {
            return Err(DmaError::FrameTooLarge.into());
        }

        // Calculate number of descriptors needed
        let desc_count = data.len().div_ceil(BUF_SIZE);

        // Check availability
        if self.tx_available() < desc_count {
            return Err(DmaError::NoDescriptorsAvailable.into());
        }

        let mut remaining = data.len();
        let mut offset = 0usize;

        // Prepare all descriptors (but don't give to DMA yet)
        for i in 0..desc_count {
            let idx = (self.tx_ring.current + i) % TX_BUFS;
            let desc = &self.tx_ring.descriptors[idx];

            // Verify ownership (should be CPU's)
            if desc.is_owned() {
                return Err(DmaError::DescriptorBusy.into());
            }

            // Calculate chunk size for this buffer
            let chunk_size = core::cmp::min(remaining, BUF_SIZE);

            // Copy data to buffer
            self.tx_buffers[idx][..chunk_size].copy_from_slice(&data[offset..offset + chunk_size]);

            // Configure descriptor
            let is_first = i == 0;
            let is_last = i == desc_count - 1;
            desc.prepare(chunk_size, is_first, is_last);

            remaining -= chunk_size;
            offset += chunk_size;
        }

        // Give descriptors to DMA in reverse order to prevent race conditions
        // (DMA might start processing before we finish setting up all descriptors)
        for i in (0..desc_count).rev() {
            let idx = (self.tx_ring.current + i) % TX_BUFS;
            self.tx_ring.descriptors[idx].set_owned();
        }

        // Advance current index
        self.tx_ring.advance_by(desc_count);

        // Issue TX poll demand to wake up DMA
        DmaRegs::tx_poll_demand();

        Ok(data.len())
    }

    /// Check if transmission is complete (all descriptors processed)
    pub fn tx_complete(&self) -> bool {
        // Check if the descriptor before current is no longer owned by DMA
        // This is a simple heuristic; for more accurate tracking, use interrupts
        let prev_idx = if self.tx_ring.current == 0 {
            TX_BUFS - 1
        } else {
            self.tx_ring.current - 1
        };
        !self.tx_ring.descriptors[prev_idx].is_owned()
    }

    /// Reclaim completed TX descriptors
    ///
    /// Returns the number of descriptors reclaimed and any error flags encountered
    pub fn tx_reclaim(&mut self) -> (usize, u32) {
        let mut reclaimed = 0;
        let mut errors = 0u32;

        // We can't reclaim past current, so we look behind
        // This is a simplified approach; a more robust implementation
        // would track a separate "clean" index
        for desc in self.tx_ring.iter() {
            if !desc.is_owned() {
                if desc.has_error() {
                    errors |= desc.error_flags();
                }
                reclaimed += 1;
            }
        }

        (reclaimed, errors)
    }

    // =========================================================================
    // RX Operations
    // =========================================================================

    /// Count free RX descriptors (owned by DMA, ready to receive)
    ///
    /// This is used for flow control to determine when to send PAUSE frames.
    pub fn rx_free_count(&self) -> usize {
        let mut count = 0;
        for desc in &self.rx_ring.descriptors {
            if desc.is_owned() {
                count += 1;
            }
        }
        count
    }

    /// Check if a complete frame is available for receiving
    pub fn rx_available(&self) -> bool {
        let desc = self.rx_ring.current();

        // If DMA still owns it, no frame available
        if desc.is_owned() {
            return false;
        }

        // Check if it's a complete frame (or at least the last descriptor of one)
        desc.is_last()
    }

    /// Get the length of the next available frame without consuming it
    ///
    /// Returns `None` if no complete frame is available
    pub fn peek_frame_length(&self) -> Option<usize> {
        let desc = self.rx_ring.current();

        if desc.is_owned() {
            return None;
        }

        if desc.has_error() {
            return None;
        }

        // For a complete single-descriptor frame
        if desc.is_first() && desc.is_last() {
            return Some(desc.payload_length());
        }

        // For multi-descriptor frames, we need to find the last descriptor
        // to get the total length
        if desc.is_first() {
            // Walk through descriptors to find the last one
            for i in 1..RX_BUFS {
                let idx = (self.rx_ring.current + i) % RX_BUFS;
                let d = &self.rx_ring.descriptors[idx];

                if d.is_owned() {
                    // Frame not complete yet
                    return None;
                }

                if d.is_last() {
                    return Some(d.payload_length());
                }
            }
        }

        None
    }

    /// Count remaining complete frames in the RX ring
    pub fn rx_frame_count(&self) -> usize {
        let mut count = 0;
        let mut idx = self.rx_ring.current;

        for _ in 0..RX_BUFS {
            let desc = &self.rx_ring.descriptors[idx];

            if desc.is_owned() {
                break;
            }

            if desc.is_last() {
                count += 1;
            }

            idx = (idx + 1) % RX_BUFS;
        }

        count
    }

    /// Receive a frame into the provided buffer
    ///
    /// Copies received frame data from DMA buffers to the provided buffer
    /// and returns the actual frame length (excluding CRC).
    ///
    /// # Arguments
    /// * `buffer` - Buffer to receive frame data
    ///
    /// # Returns
    /// * `Ok(len)` - Number of bytes received
    /// * `Err(Error::Io(IoError::BufferTooSmall))` - Buffer too small for frame
    /// * `Err(Error::Io(IoError::IncompleteFrame))` - No complete frame available
    /// * `Err(Error::Io(IoError::FrameError))` - Frame has receive errors
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize> {
        // Find frame start (should be at current index)
        let first_desc = self.rx_ring.current();

        if first_desc.is_owned() {
            return Err(IoError::IncompleteFrame.into());
        }

        // For a single-descriptor frame
        if first_desc.is_first() && first_desc.is_last() {
            if first_desc.has_error() {
                // Recycle and skip this frame
                first_desc.recycle();
                self.rx_ring.advance();
                DmaRegs::rx_poll_demand();
                return Err(IoError::FrameError.into());
            }

            let frame_len = first_desc.payload_length();

            if buffer.len() < frame_len {
                // Recycle and skip
                first_desc.recycle();
                self.rx_ring.advance();
                DmaRegs::rx_poll_demand();
                return Err(IoError::BufferTooSmall.into());
            }

            // Copy data
            let idx = self.rx_ring.current_index();
            buffer[..frame_len].copy_from_slice(&self.rx_buffers[idx][..frame_len]);

            // Recycle descriptor
            first_desc.recycle();
            self.rx_ring.advance();
            DmaRegs::rx_poll_demand();

            return Ok(frame_len);
        }

        // Multi-descriptor frame
        if !first_desc.is_first() {
            // We're in the middle of a frame somehow - flush and try again
            self.flush_rx_frame();
            return Err(IoError::IncompleteFrame.into());
        }

        // Check for errors on first descriptor
        if first_desc.has_error() {
            self.flush_rx_frame();
            return Err(IoError::FrameError.into());
        }

        // Find the last descriptor and calculate total length
        let mut frame_len = 0usize;
        let mut desc_count = 0usize;
        let mut last_idx = self.rx_ring.current_index();

        for i in 0..RX_BUFS {
            let idx = (self.rx_ring.current_index() + i) % RX_BUFS;
            let desc = &self.rx_ring.descriptors[idx];

            if desc.is_owned() {
                // Frame not complete
                return Err(IoError::IncompleteFrame.into());
            }

            desc_count += 1;
            last_idx = idx;

            if desc.is_last() {
                frame_len = desc.payload_length();
                break;
            }
        }

        if buffer.len() < frame_len {
            self.flush_rx_frame();
            return Err(IoError::BufferTooSmall.into());
        }

        // Copy data from all descriptors
        let mut copied = 0usize;

        for i in 0..desc_count {
            let idx = (self.rx_ring.current_index() + i) % RX_BUFS;
            let desc = &self.rx_ring.descriptors[idx];

            // Calculate how much to copy from this buffer
            let buf_data_len = if idx == last_idx {
                // Last descriptor - copy only what's needed
                frame_len - copied
            } else {
                // Full buffer
                BUF_SIZE
            };

            let copy_len = core::cmp::min(buf_data_len, frame_len - copied);

            if copy_len > 0 {
                buffer[copied..copied + copy_len]
                    .copy_from_slice(&self.rx_buffers[idx][..copy_len]);
                copied += copy_len;
            }

            // Recycle descriptor
            desc.recycle();
        }

        // Advance past all consumed descriptors
        self.rx_ring.advance_by(desc_count);
        DmaRegs::rx_poll_demand();

        Ok(frame_len)
    }

    /// Flush (discard) the current RX frame
    ///
    /// Used to skip frames with errors or when buffer is too small.
    pub fn flush_rx_frame(&mut self) {
        loop {
            let desc = self.rx_ring.current();

            if desc.is_owned() {
                break;
            }

            let is_last = desc.is_last();
            desc.recycle();
            self.rx_ring.advance();

            if is_last {
                break;
            }
        }

        DmaRegs::rx_poll_demand();
    }

    // =========================================================================
    // Statistics and Debug
    // =========================================================================

    /// Get RX ring base address (for debugging)
    pub fn rx_ring_base(&self) -> u32 {
        self.rx_ring.base_addr_u32()
    }

    /// Get TX ring base address (for debugging)
    pub fn tx_ring_base(&self) -> u32 {
        self.tx_ring.base_addr_u32()
    }

    /// Get current RX index (for debugging)
    pub fn rx_current_index(&self) -> usize {
        self.rx_ring.current_index()
    }

    /// Get current TX index (for debugging)
    pub fn tx_current_index(&self) -> usize {
        self.tx_ring.current_index()
    }

    /// Get a reference to the RX buffer at the given index
    pub fn rx_buffer(&self, index: usize) -> &[u8; BUF_SIZE] {
        &self.rx_buffers[index % RX_BUFS]
    }

    /// Get a reference to the TX buffer at the given index
    pub fn tx_buffer(&self, index: usize) -> &[u8; BUF_SIZE] {
        &self.tx_buffers[index % TX_BUFS]
    }
}

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Default
    for DmaEngine<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    fn default() -> Self {
        Self::new()
    }
}

// Safety: DmaEngine can be shared between threads when properly synchronized
// The caller must ensure exclusive access during init/transmit/receive
unsafe impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Sync
    for DmaEngine<RX_BUFS, TX_BUFS, BUF_SIZE>
{
}

unsafe impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> Send
    for DmaEngine<RX_BUFS, TX_BUFS, BUF_SIZE>
{
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_usage() {
        let size = DmaEngine::<10, 10, 1600>::memory_usage();
        // 10 * 32 + 10 * 32 + 10 * 1600 + 10 * 1600 = 640 + 32000 = 32640
        assert!(size > 32000);
        assert!(size < 40000);
    }

    #[test]
    fn test_descriptor_ring_advance() {
        let mut ring: DescriptorRing<u32, 4> = DescriptorRing {
            descriptors: [0, 1, 2, 3],
            current: 0,
        };

        assert_eq!(ring.current_index(), 0);
        ring.advance();
        assert_eq!(ring.current_index(), 1);
        ring.advance();
        assert_eq!(ring.current_index(), 2);
        ring.advance();
        assert_eq!(ring.current_index(), 3);
        ring.advance();
        assert_eq!(ring.current_index(), 0); // Wrapped
    }

    // =========================================================================
    // DescriptorRing Tests
    // =========================================================================

    #[test]
    fn descriptor_ring_from_array() {
        let ring = DescriptorRing::from_array([10u32, 20, 30, 40]);
        assert_eq!(ring.len(), 4);
        assert_eq!(ring.current_index(), 0);
    }

    #[test]
    fn descriptor_ring_len() {
        let ring: DescriptorRing<u8, 8> = DescriptorRing {
            descriptors: [0; 8],
            current: 0,
        };
        assert_eq!(ring.len(), 8);
    }

    #[test]
    fn descriptor_ring_is_empty_false_for_non_zero_size() {
        let ring: DescriptorRing<u8, 4> = DescriptorRing {
            descriptors: [0; 4],
            current: 0,
        };
        assert!(!ring.is_empty());
    }

    #[test]
    fn descriptor_ring_is_empty_true_for_zero_size() {
        let ring: DescriptorRing<u8, 0> = DescriptorRing {
            descriptors: [],
            current: 0,
        };
        assert!(ring.is_empty());
    }

    #[test]
    fn descriptor_ring_current_returns_reference() {
        let ring = DescriptorRing::from_array([100u32, 200, 300]);
        assert_eq!(*ring.current(), 100);
    }

    #[test]
    fn descriptor_ring_current_mut_allows_modification() {
        let mut ring = DescriptorRing::from_array([100u32, 200, 300]);
        *ring.current_mut() = 999;
        assert_eq!(*ring.current(), 999);
    }

    #[test]
    fn descriptor_ring_get_by_index() {
        let ring = DescriptorRing::from_array([10u32, 20, 30, 40]);
        assert_eq!(*ring.get(0), 10);
        assert_eq!(*ring.get(1), 20);
        assert_eq!(*ring.get(2), 30);
        assert_eq!(*ring.get(3), 40);
    }

    #[test]
    fn descriptor_ring_get_wraps_index() {
        let ring = DescriptorRing::from_array([10u32, 20, 30, 40]);
        // Index 4 should wrap to 0
        assert_eq!(*ring.get(4), 10);
        assert_eq!(*ring.get(5), 20);
        assert_eq!(*ring.get(8), 10);
    }

    #[test]
    fn descriptor_ring_get_mut_by_index() {
        let mut ring = DescriptorRing::from_array([10u32, 20, 30, 40]);
        *ring.get_mut(2) = 999;
        assert_eq!(*ring.get(2), 999);
    }

    #[test]
    fn descriptor_ring_at_offset() {
        let mut ring = DescriptorRing::from_array([10u32, 20, 30, 40]);
        ring.advance(); // current = 1
        assert_eq!(*ring.at_offset(0), 20); // current
        assert_eq!(*ring.at_offset(1), 30); // current + 1
        assert_eq!(*ring.at_offset(2), 40); // current + 2
        assert_eq!(*ring.at_offset(3), 10); // wraps to 0
    }

    #[test]
    fn descriptor_ring_advance_by() {
        let mut ring = DescriptorRing::from_array([0u32; 8]);
        assert_eq!(ring.current_index(), 0);
        ring.advance_by(3);
        assert_eq!(ring.current_index(), 3);
        ring.advance_by(3);
        assert_eq!(ring.current_index(), 6);
        ring.advance_by(5);
        assert_eq!(ring.current_index(), 3); // (6 + 5) % 8 = 3
    }

    #[test]
    fn descriptor_ring_reset() {
        let mut ring = DescriptorRing::from_array([0u32; 4]);
        ring.advance();
        ring.advance();
        assert_eq!(ring.current_index(), 2);
        ring.reset();
        assert_eq!(ring.current_index(), 0);
    }

    #[test]
    fn descriptor_ring_base_addr() {
        let ring = DescriptorRing::from_array([10u32, 20, 30]);
        let ptr = ring.base_addr();
        assert!(!ptr.is_null());
        // Pointer should point to first element
        unsafe {
            assert_eq!(*ptr, 10);
        }
    }

    #[test]
    fn descriptor_ring_base_addr_u32() {
        let ring = DescriptorRing::from_array([10u32, 20, 30]);
        let addr = ring.base_addr_u32();
        // Should be non-zero (valid address)
        assert!(addr != 0);
        // Should equal the pointer cast
        assert_eq!(addr, ring.base_addr() as u32);
    }

    #[test]
    fn descriptor_ring_iter() {
        let ring = DescriptorRing::from_array([10u32, 20, 30, 40]);
        let mut iter = ring.iter();
        assert_eq!(iter.next(), Some(&10));
        assert_eq!(iter.next(), Some(&20));
        assert_eq!(iter.next(), Some(&30));
        assert_eq!(iter.next(), Some(&40));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn descriptor_ring_iter_mut() {
        let mut ring = DescriptorRing::from_array([1u32, 2, 3, 4]);
        for val in ring.iter_mut() {
            *val *= 10;
        }
        assert_eq!(*ring.get(0), 10);
        assert_eq!(*ring.get(1), 20);
        assert_eq!(*ring.get(2), 30);
        assert_eq!(*ring.get(3), 40);
    }

    // =========================================================================
    // DmaEngine Tests
    // =========================================================================

    #[test]
    fn dma_engine_new_is_not_initialized() {
        let dma: DmaEngine<4, 4, 1600> = DmaEngine::new();
        assert!(!dma.is_initialized());
    }

    #[test]
    fn dma_engine_memory_usage_scales_with_buffers() {
        let small = DmaEngine::<2, 2, 512>::memory_usage();
        let large = DmaEngine::<10, 10, 1600>::memory_usage();
        assert!(large > small);
    }

    #[test]
    fn dma_engine_memory_usage_scales_with_buffer_size() {
        let small = DmaEngine::<4, 4, 512>::memory_usage();
        let large = DmaEngine::<4, 4, 2048>::memory_usage();
        assert!(large > small);
    }

    #[test]
    fn dma_engine_tx_ctrl_flags_default() {
        let dma: DmaEngine<4, 4, 1600> = DmaEngine::new();
        assert_eq!(dma.tx_ctrl_flags(), 0);
    }

    #[test]
    fn dma_engine_set_tx_ctrl_flags() {
        let mut dma: DmaEngine<4, 4, 1600> = DmaEngine::new();
        dma.set_tx_ctrl_flags(0x1234);
        assert_eq!(dma.tx_ctrl_flags(), 0x1234);
    }

    // =========================================================================
    // Mock Test Utilities (imported from testing)
    // =========================================================================

    // Use the shared MockDescriptor from testing module
    use crate::testing::MockDescriptor;

    // =========================================================================
    // DescriptorRing with MockDescriptor Tests
    // =========================================================================

    #[test]
    fn mock_descriptor_ring_basic_operations() {
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 0,
        };

        // Initially all descriptors are not owned
        for desc in ring.iter() {
            assert!(!desc.is_owned());
        }

        // Set ownership on first two descriptors
        ring.get_mut(0).set_owned();
        ring.get_mut(1).set_owned();

        assert!(ring.get(0).is_owned());
        assert!(ring.get(1).is_owned());
        assert!(!ring.get(2).is_owned());
        assert!(!ring.get(3).is_owned());
    }

    #[test]
    fn mock_descriptor_ring_simulate_rx_flow() {
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 0,
        };

        // Give all descriptors to DMA
        for desc in ring.iter_mut() {
            desc.set_owned();
        }

        // Simulate DMA receiving a frame on descriptor 0
        ring.get_mut(0).simulate_receive(1500);

        // Current descriptor should now be available
        assert!(!ring.current().is_owned());
        assert!(ring.current().is_first());
        assert!(ring.current().is_last());
        assert_eq!(ring.current().frame_length(), 1500);
    }

    #[test]
    fn mock_descriptor_ring_simulate_multi_desc_frame() {
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 0,
        };

        // Simulate a frame spanning 2 descriptors
        ring.get_mut(0).owned = false;
        ring.get_mut(0).first = true;
        ring.get_mut(0).last = false;
        ring.get_mut(0).frame_len = 1600;

        ring.get_mut(1).owned = false;
        ring.get_mut(1).first = false;
        ring.get_mut(1).last = true;
        ring.get_mut(1).frame_len = 2048; // Total frame length

        assert!(ring.get(0).is_first());
        assert!(!ring.get(0).is_last());
        assert!(!ring.get(1).is_first());
        assert!(ring.get(1).is_last());
    }

    // =========================================================================
    // Descriptor Ring Ownership Tracking Tests
    // =========================================================================

    #[test]
    fn descriptor_ring_count_owned() {
        let mut ring: DescriptorRing<MockDescriptor, 8> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 8],
            current: 0,
        };

        // Helper function to count owned descriptors
        fn count_owned(ring: &DescriptorRing<MockDescriptor, 8>) -> usize {
            ring.iter().filter(|d| d.is_owned()).count()
        }

        assert_eq!(count_owned(&ring), 0);

        ring.get_mut(0).set_owned();
        ring.get_mut(2).set_owned();
        ring.get_mut(5).set_owned();

        assert_eq!(count_owned(&ring), 3);

        ring.get_mut(0).clear_owned();
        assert_eq!(count_owned(&ring), 2);
    }

    #[test]
    fn descriptor_ring_find_next_available() {
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 1, // Start at index 1
        };

        // Mark all as owned except index 3
        for desc in ring.iter_mut() {
            desc.set_owned();
        }
        ring.get_mut(3).clear_owned();

        // Search for next available descriptor
        let mut found_offset = None;
        for offset in 0..4 {
            if !ring.at_offset(offset).is_owned() {
                found_offset = Some(offset);
                break;
            }
        }

        // From current=1, offset 2 leads to index 3
        assert_eq!(found_offset, Some(2));
    }

    // =========================================================================
    // Buffer Size and Alignment Tests
    // =========================================================================

    #[test]
    fn dma_engine_buffer_sizes() {
        // Verify that buffer sizes are correctly represented
        let _small: DmaEngine<2, 2, 512> = DmaEngine::new();
        let _medium: DmaEngine<4, 4, 1600> = DmaEngine::new();
        let _large: DmaEngine<8, 8, 2048> = DmaEngine::new();

        // Memory usage should increase with size
        assert!(DmaEngine::<2, 2, 512>::memory_usage() < DmaEngine::<4, 4, 1600>::memory_usage());
        assert!(DmaEngine::<4, 4, 1600>::memory_usage() < DmaEngine::<8, 8, 2048>::memory_usage());
    }

    #[test]
    fn dma_engine_buffer_access() {
        let dma: DmaEngine<4, 4, 256> = DmaEngine::new();

        // Should be able to access all buffers without panic
        for i in 0..4 {
            let _rx_buf = dma.rx_buffer(i);
            let _tx_buf = dma.tx_buffer(i);
        }

        // Index wrapping should work
        let buf0 = dma.rx_buffer(0);
        let buf4 = dma.rx_buffer(4); // Should wrap to 0
        assert_eq!(buf0.as_ptr(), buf4.as_ptr());
    }

    #[test]
    fn dma_engine_ring_base_addresses() {
        let dma: DmaEngine<4, 4, 1600> = DmaEngine::new();

        // Base addresses should be non-zero
        assert_ne!(dma.rx_ring_base(), 0);
        assert_ne!(dma.tx_ring_base(), 0);

        // RX and TX rings should have different addresses
        assert_ne!(dma.rx_ring_base(), dma.tx_ring_base());
    }

    #[test]
    fn dma_engine_initial_indices() {
        let dma: DmaEngine<4, 4, 1600> = DmaEngine::new();

        // Initial indices should be 0
        assert_eq!(dma.rx_current_index(), 0);
        assert_eq!(dma.tx_current_index(), 0);
    }

    // =========================================================================
    // Frame Processing Simulation Tests
    // =========================================================================

    /// Helper function to count available descriptors in a mock ring
    fn count_available(ring: &DescriptorRing<MockDescriptor, 4>) -> usize {
        ring.iter().filter(|d| !d.is_owned()).count()
    }

    #[test]
    fn simulate_tx_submission_flow() {
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 0,
        };

        // All descriptors start as available (not owned by DMA)
        assert_eq!(count_available(&ring), 4);

        // Submit 3 frames (give to DMA)
        for _ in 0..3 {
            assert!(
                !ring.current().is_owned(),
                "Current descriptor should be available"
            );
            ring.current_mut().set_owned();
            ring.advance();
        }

        // Now 3 are owned, 1 available
        assert_eq!(count_available(&ring), 1);
        assert_eq!(ring.current_index(), 3);
    }

    #[test]
    fn simulate_tx_completion_flow() {
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 0,
        };

        // Submit all 4 descriptors
        for desc in ring.iter_mut() {
            desc.set_owned();
        }
        ring.advance_by(4); // Ring wraps, current = 0

        // Simulate DMA completing transmission on first 2
        ring.get_mut(0).clear_owned();
        ring.get_mut(1).clear_owned();

        // Reclaim completed descriptors
        let mut reclaimed = 0;
        let mut reclaim_idx = 0usize;
        while !ring.get(reclaim_idx).is_owned() && reclaimed < 4 {
            reclaimed += 1;
            reclaim_idx = (reclaim_idx + 1) % 4;
            // Stop if we've checked all or hit an owned descriptor
            if reclaim_idx == 2 {
                break;
            }
        }

        assert_eq!(reclaimed, 2);
    }

    #[test]
    fn simulate_rx_receive_flow() {
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 0,
        };

        // Give all descriptors to DMA for receiving
        for desc in ring.iter_mut() {
            desc.set_owned();
        }

        // Simulate DMA receiving frames on descriptors 0, 1
        ring.get_mut(0).simulate_receive(64);
        ring.get_mut(1).simulate_receive(1500);

        // Process received frames using fixed-size array
        let mut frames_received: [usize; 4] = [0; 4];
        let mut frame_count = 0;

        while !ring.current().is_owned() && frame_count < 4 {
            let frame_len = ring.current().frame_length();
            if ring.current().is_first() && ring.current().is_last() {
                frames_received[frame_count] = frame_len;
                frame_count += 1;
            }
            // Return to DMA for reuse
            ring.current_mut().set_owned();
            ring.advance();

            // Safety: Don't process forever
            if ring.current_index() == 0 && frame_count > 0 {
                break;
            }
        }

        assert_eq!(frame_count, 2);
        assert_eq!(frames_received[0], 64);
        assert_eq!(frames_received[1], 1500);
    }

    #[test]
    fn simulate_rx_error_handling() {
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 0,
        };

        // Give to DMA
        for desc in ring.iter_mut() {
            desc.set_owned();
        }

        // Simulate error on first descriptor
        ring.get_mut(0).simulate_error();

        // Check error detection
        assert!(ring.current().has_error());

        // Should still be able to recycle the descriptor
        ring.current_mut().set_owned();
        ring.advance();
    }

    // =========================================================================
    // Ring Wraparound Tests
    // =========================================================================

    #[test]
    fn descriptor_ring_wraparound_stress() {
        let mut ring: DescriptorRing<u32, 7> = DescriptorRing {
            descriptors: [0; 7],
            current: 0,
        };

        // Advance many times and verify wraparound is correct
        for i in 0..100 {
            assert_eq!(ring.current_index(), i % 7);
            ring.advance();
        }
    }

    #[test]
    fn descriptor_ring_advance_by_wraparound() {
        let mut ring: DescriptorRing<u32, 5> = DescriptorRing {
            descriptors: [0; 5],
            current: 0,
        };

        ring.advance_by(3);
        assert_eq!(ring.current_index(), 3);

        ring.advance_by(4);
        assert_eq!(ring.current_index(), 2); // (3 + 4) % 5 = 2

        ring.advance_by(10);
        assert_eq!(ring.current_index(), 2); // (2 + 10) % 5 = 2
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[test]
    fn descriptor_ring_single_element() {
        let mut ring: DescriptorRing<MockDescriptor, 1> = DescriptorRing {
            descriptors: [MockDescriptor::new()],
            current: 0,
        };

        assert_eq!(ring.len(), 1);
        assert_eq!(ring.current_index(), 0);

        ring.advance();
        assert_eq!(ring.current_index(), 0); // Still 0, wraps immediately

        ring.get_mut(0).set_owned();
        assert!(ring.current().is_owned());
    }

    #[test]
    fn mock_rx_back_pressure() {
        // Simulate a scenario where all descriptors are filled before processing
        let mut ring: DescriptorRing<MockDescriptor, 4> = DescriptorRing {
            descriptors: [MockDescriptor::new(); 4],
            current: 0,
        };

        // Give all to DMA
        for desc in ring.iter_mut() {
            desc.set_owned();
        }

        // DMA fills all descriptors
        for (i, desc) in ring.iter_mut().enumerate() {
            desc.simulate_receive(100 + i * 100);
        }

        // No more descriptors available for DMA (simulating back pressure)
        let available_for_dma = ring.iter().filter(|d| d.is_owned()).count();
        assert_eq!(available_for_dma, 0);

        // Process all frames and return descriptors to DMA
        for _ in 0..4 {
            assert!(ring.current().is_first());
            ring.current_mut().set_owned();
            ring.advance();
        }

        // All descriptors now available again
        let available_for_dma = ring.iter().filter(|d| d.is_owned()).count();
        assert_eq!(available_for_dma, 4);
    }

    #[test]
    fn dma_engine_default_trait() {
        let dma1: DmaEngine<4, 4, 1600> = DmaEngine::new();
        let dma2: DmaEngine<4, 4, 1600> = DmaEngine::default();

        // Both should have same initial state
        assert!(!dma1.is_initialized());
        assert!(!dma2.is_initialized());
        assert_eq!(dma1.tx_ctrl_flags(), dma2.tx_ctrl_flags());
    }
}
