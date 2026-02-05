//! DMA engine managing TX/RX descriptor rings and buffers.

use super::descriptor::{RxDescriptor, TxDescriptor};
use super::ring::DescriptorRing;
use crate::driver::error::{DmaError, IoError, Result};
use crate::internal::register::dma::DmaRegs;

#[cfg(feature = "log")]
use log::warn;

#[cfg(feature = "log")]
fn log_rx_error(desc: &RxDescriptor) {
    use crate::internal::dma::descriptor::bits::rdes0;

    let raw = desc.raw_rdes0();
    let error_flags = raw & (rdes0::ALL_ERRORS | rdes0::SA_FILTER_FAIL | rdes0::DA_FILTER_FAIL);
    let sa_fail = (raw & rdes0::SA_FILTER_FAIL) != 0;
    let da_fail = (raw & rdes0::DA_FILTER_FAIL) != 0;

    warn!(
        "RX frame error: rdes0=0x{:08x} flags=0x{:08x} sa_filter_fail={} da_filter_fail={}",
        raw, error_flags, sa_fail, da_fail
    );
}
/// DMA Engine with statically allocated buffers.
///
/// # Type Parameters
/// * `RX_BUFS` - Number of receive buffers/descriptors
/// * `TX_BUFS` - Number of transmit buffers/descriptors
/// * `BUF_SIZE` - Size of each buffer in bytes (>= 1600 for standard frames)
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
    /// Create a new DMA engine with zeroed buffers. Const-compatible.
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

    /// Total memory usage in bytes.
    #[must_use]
    pub const fn memory_usage() -> usize {
        let rx_desc_size = RX_BUFS * RxDescriptor::SIZE;
        let tx_desc_size = TX_BUFS * TxDescriptor::SIZE;
        let rx_buf_size = RX_BUFS * BUF_SIZE;
        let tx_buf_size = TX_BUFS * BUF_SIZE;
        rx_desc_size + tx_desc_size + rx_buf_size + tx_buf_size
    }

    /// Initialize descriptor chains and DMA registers.
    /// Must be called before any DMA operations.
    pub fn init(&mut self) {
        for i in 0..RX_BUFS {
            let next_idx = (i + 1) % RX_BUFS;
            let buffer_ptr = self.rx_buffers[i].as_mut_ptr();
            let next_desc = &self.rx_ring.descriptors[next_idx] as *const RxDescriptor;
            self.rx_ring.descriptors[i].setup_chained(buffer_ptr, BUF_SIZE, next_desc);
        }

        for i in 0..TX_BUFS {
            let next_idx = (i + 1) % TX_BUFS;
            let buffer_ptr = self.tx_buffers[i].as_ptr();
            let next_desc = &self.tx_ring.descriptors[next_idx] as *const TxDescriptor;
            self.tx_ring.descriptors[i].setup_chained(buffer_ptr, next_desc);
        }

        self.rx_ring.reset();
        self.tx_ring.reset();
        DmaRegs::set_rx_desc_list_addr(self.rx_ring.base_addr_u32());
        DmaRegs::set_tx_desc_list_addr(self.tx_ring.base_addr_u32());
        self.initialized = true;
    }

    /// Reset to initial state. Caller should stop DMA first.
    pub fn reset(&mut self) {
        for i in 0..RX_BUFS {
            self.rx_ring.descriptors[i].recycle();
        }
        for i in 0..TX_BUFS {
            self.tx_ring.descriptors[i].reset();
        }
        self.rx_ring.reset();
        self.tx_ring.reset();
        DmaRegs::set_rx_desc_list_addr(self.rx_ring.base_addr_u32());
        DmaRegs::set_tx_desc_list_addr(self.tx_ring.base_addr_u32());
    }

    /// Check if the DMA engine has been initialized
    #[inline(always)]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Set TX control flags (checksum offload, etc).
    pub fn set_tx_ctrl_flags(&mut self, flags: u32) {
        self.tx_ctrl_flags = flags;
    }

    /// Get the current TX control flags
    #[inline(always)]
    pub fn tx_ctrl_flags(&self) -> u32 {
        self.tx_ctrl_flags
    }

    /// Count available TX descriptors (not owned by DMA).
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

    /// Check if enough descriptors available for frame of given size.
    pub fn can_transmit(&self, len: usize) -> bool {
        if len == 0 || len > BUF_SIZE * TX_BUFS {
            return false;
        }
        let needed = len.div_ceil(BUF_SIZE);
        self.tx_available() >= needed
    }

    /// Transmit a frame. Supports scatter-gather for large frames.
    pub fn transmit(&mut self, data: &[u8]) -> Result<usize> {
        if data.is_empty() {
            return Err(DmaError::InvalidLength.into());
        }

        let total_capacity = BUF_SIZE * TX_BUFS;
        if data.len() > total_capacity {
            return Err(DmaError::FrameTooLarge.into());
        }

        let desc_count = data.len().div_ceil(BUF_SIZE);
        if self.tx_available() < desc_count {
            return Err(DmaError::NoDescriptorsAvailable.into());
        }

        let mut remaining = data.len();
        let mut offset = 0usize;

        // Prepare descriptors
        for i in 0..desc_count {
            let idx = (self.tx_ring.current + i) % TX_BUFS;
            let desc = &self.tx_ring.descriptors[idx];

            if desc.is_owned() {
                return Err(DmaError::DescriptorBusy.into());
            }

            let chunk_size = core::cmp::min(remaining, BUF_SIZE);
            self.tx_buffers[idx][..chunk_size].copy_from_slice(&data[offset..offset + chunk_size]);
            desc.prepare(chunk_size, i == 0, i == desc_count - 1);

            remaining -= chunk_size;
            offset += chunk_size;
        }

        // Give to DMA in reverse order (prevents race)
        for i in (0..desc_count).rev() {
            let idx = (self.tx_ring.current + i) % TX_BUFS;
            self.tx_ring.descriptors[idx].set_owned();
        }

        self.tx_ring.advance_by(desc_count);
        DmaRegs::tx_poll_demand();
        Ok(data.len())
    }

    /// Check if previous transmission completed.
    pub fn tx_complete(&self) -> bool {
        let prev_idx = if self.tx_ring.current == 0 {
            TX_BUFS - 1
        } else {
            self.tx_ring.current - 1
        };
        !self.tx_ring.descriptors[prev_idx].is_owned()
    }

    /// Reclaim completed TX descriptors. Returns (count, error_flags).
    pub fn tx_reclaim(&mut self) -> (usize, u32) {
        let mut reclaimed = 0;
        let mut errors = 0u32;

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

    /// Count free RX descriptors (owned by DMA).
    pub fn rx_free_count(&self) -> usize {
        let mut count = 0;
        for desc in &self.rx_ring.descriptors {
            if desc.is_owned() {
                count += 1;
            }
        }
        count
    }

    /// Check if a complete frame is available.
    pub fn rx_available(&self) -> bool {
        let desc = self.rx_ring.current();
        !desc.is_owned() && desc.is_last()
    }

    /// Peek next frame length without consuming.
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

    /// Receive a frame into buffer. Returns length excluding CRC.
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let first_desc = self.rx_ring.current();

        if first_desc.is_owned() {
            return Err(IoError::IncompleteFrame.into());
        }

        // Single-descriptor frame (common case)
        if first_desc.is_first() && first_desc.is_last() {
            if first_desc.has_error() {
                #[cfg(feature = "log")]
                log_rx_error(first_desc);
                first_desc.recycle();
                self.rx_ring.advance();
                DmaRegs::rx_poll_demand();
                return Err(IoError::FrameError.into());
            }

            let frame_len = first_desc.payload_length();
            if buffer.len() < frame_len {
                first_desc.recycle();
                self.rx_ring.advance();
                DmaRegs::rx_poll_demand();
                return Err(IoError::BufferTooSmall.into());
            }

            let idx = self.rx_ring.current_index();
            buffer[..frame_len].copy_from_slice(&self.rx_buffers[idx][..frame_len]);
            first_desc.recycle();
            self.rx_ring.advance();
            DmaRegs::rx_poll_demand();
            return Ok(frame_len);
        }

        // Multi-descriptor frame
        if !first_desc.is_first() {
            self.flush_rx_frame();
            return Err(IoError::IncompleteFrame.into());
        }

        if first_desc.has_error() {
            #[cfg(feature = "log")]
            log_rx_error(first_desc);
            self.flush_rx_frame();
            return Err(IoError::FrameError.into());
        }

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
            let buf_data_len = if idx == last_idx {
                frame_len - copied
            } else {
                BUF_SIZE
            };
            let copy_len = core::cmp::min(buf_data_len, frame_len - copied);

            if copy_len > 0 {
                buffer[copied..copied + copy_len]
                    .copy_from_slice(&self.rx_buffers[idx][..copy_len]);
                copied += copy_len;
            }
            desc.recycle();
        }

        self.rx_ring.advance_by(desc_count);
        DmaRegs::rx_poll_demand();

        Ok(frame_len)
    }

    /// Discard current RX frame (for errors or small buffer).
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

    /// RX ring base address (for debugging).
    pub fn rx_ring_base(&self) -> u32 {
        self.rx_ring.base_addr_u32()
    }

    /// TX ring base address (for debugging).
    pub fn tx_ring_base(&self) -> u32 {
        self.tx_ring.base_addr_u32()
    }

    /// Current RX index (for debugging).
    pub fn rx_current_index(&self) -> usize {
        self.rx_ring.current_index()
    }

    /// Current TX index (for debugging).
    pub fn tx_current_index(&self) -> usize {
        self.tx_ring.current_index()
    }

    /// RX buffer at index.
    pub fn rx_buffer(&self, index: usize) -> &[u8; BUF_SIZE] {
        &self.rx_buffers[index % RX_BUFS]
    }

    /// TX buffer at index.
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
    use crate::testing::MockDescriptor;

    #[test]
    fn test_memory_usage() {
        let size = DmaEngine::<10, 10, 1600>::memory_usage();
        // 10 * 32 + 10 * 32 + 10 * 1600 + 10 * 1600 = 640 + 32000 = 32640
        assert!(size > 32000);
        assert!(size < 40000);
    }

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
    // Frame Processing Simulation Tests (using DescriptorRing with MockDescriptor)
    // =========================================================================

    use super::super::ring::DescriptorRing;

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
