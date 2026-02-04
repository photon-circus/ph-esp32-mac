//! Generic circular ring buffer for DMA descriptors.

/// Circular descriptor ring with wraparound index.
pub struct DescriptorRing<D, const N: usize> {
    /// Array of descriptors
    pub(super) descriptors: [D; N],
    /// Current index for processing
    pub(super) current: usize,
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
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockDescriptor;

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
}
