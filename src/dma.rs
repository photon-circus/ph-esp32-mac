//! DMA Engine
//!
//! This module provides the DMA engine for managing TX and RX descriptor rings
//! and buffer transfers. All memory is statically allocated using const generics.

use crate::descriptor::{RxDescriptor, TxDescriptor};
use crate::error::{DmaError, IoError, Result};
use crate::register::dma::DmaRegs;

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
}
