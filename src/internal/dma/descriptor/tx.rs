//! TX DMA descriptor for frame transmission.

use super::VolatileCell;
use super::bits::{tdes0, tdes1};

/// TX DMA descriptor (32 bytes on ESP32, 64 bytes on ESP32-P4).
#[repr(C)]
#[cfg_attr(not(feature = "esp32p4"), repr(align(4)))]
#[cfg_attr(feature = "esp32p4", repr(align(64)))]
pub struct TxDescriptor {
    /// TDES0: Status and control bits
    tdes0: VolatileCell<u32>,
    /// TDES1: Buffer sizes
    tdes1: VolatileCell<u32>,
    /// TDES2: Buffer 1 address
    buffer1_addr: VolatileCell<u32>,
    /// TDES3: Buffer 2 / Next descriptor address (in chained mode)
    buffer2_next_desc: VolatileCell<u32>,
    /// Reserved / Extended status (ESP32-P4)
    _reserved1: u32,
    /// Reserved
    _reserved2: u32,
    /// Timestamp low (when timestamping enabled)
    timestamp_low: VolatileCell<u32>,
    /// Timestamp high (when timestamping enabled)
    timestamp_high: VolatileCell<u32>,
}

#[allow(dead_code)]
impl TxDescriptor {
    /// Size of the descriptor in bytes
    #[cfg(not(feature = "esp32p4"))]
    pub const SIZE: usize = 32;

    /// Size of the descriptor in bytes (ESP32-P4 with cache alignment)
    #[cfg(feature = "esp32p4")]
    pub const SIZE: usize = 64;

    /// Create a new zeroed TX descriptor.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tdes0: VolatileCell::new(0),
            tdes1: VolatileCell::new(0),
            buffer1_addr: VolatileCell::new(0),
            buffer2_next_desc: VolatileCell::new(0),
            _reserved1: 0,
            _reserved2: 0,
            timestamp_low: VolatileCell::new(0),
            timestamp_high: VolatileCell::new(0),
        }
    }

    /// Initialize descriptor for chained mode.
    pub fn setup_chained(&self, buffer: *const u8, next_desc: *const TxDescriptor) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc.set(next_desc as u32);
        self.tdes0.set(tdes0::SECOND_ADDR_CHAINED);
        self.tdes1.set(0);
    }

    /// Initialize as end of ring (last descriptor wraps to first).
    pub fn setup_end_of_ring(&self, buffer: *const u8, first_desc: *const TxDescriptor) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc.set(first_desc as u32);
        self.tdes0
            .set(tdes0::SECOND_ADDR_CHAINED | tdes0::TX_END_OF_RING);
        self.tdes1.set(0);
    }

    /// Check if descriptor is owned by DMA.
    #[inline(always)]
    #[must_use]
    pub fn is_owned(&self) -> bool {
        (self.tdes0.get() & tdes0::OWN) != 0
    }

    /// Give ownership to DMA for transmission.
    #[inline(always)]
    pub fn set_owned(&self) {
        self.tdes0.update(|v| v | tdes0::OWN);
    }

    /// Take ownership from DMA for CPU use.
    #[inline(always)]
    pub fn clear_owned(&self) {
        self.tdes0.update(|v| v & !tdes0::OWN);
    }

    /// Prepare descriptor for transmission with segment flags.
    pub fn prepare(&self, len: usize, first: bool, last: bool) {
        let mut flags = tdes0::SECOND_ADDR_CHAINED;

        if first {
            flags |= tdes0::FIRST_SEGMENT;
        }
        if last {
            flags |= tdes0::LAST_SEGMENT | tdes0::INTERRUPT_ON_COMPLETE;
        }

        self.tdes1.set((len as u32) & tdes1::BUFFER1_SIZE_MASK);

        // Set flags (but not OWN yet)
        self.tdes0.set(flags);
    }

    /// Prepare and submit to DMA in one operation.
    pub fn prepare_and_submit(&self, len: usize, first: bool, last: bool) {
        self.prepare(len, first, last);
        self.set_owned();
    }

    /// Set checksum insertion mode.
    pub fn set_checksum_mode(&self, mode: u32) {
        self.tdes0.update(|v| {
            (v & !tdes0::CHECKSUM_INSERT_MASK)
                | ((mode << tdes0::CHECKSUM_INSERT_SHIFT) & tdes0::CHECKSUM_INSERT_MASK)
        });
    }

    /// Enable timestamp capture for this frame.
    pub fn enable_timestamp(&self) {
        self.tdes0.update(|v| v | tdes0::TX_TIMESTAMP_EN);
    }

    /// Check if transmission had errors.
    #[inline(always)]
    #[must_use]
    pub fn has_error(&self) -> bool {
        (self.tdes0.get() & tdes0::ERR_SUMMARY) != 0
    }

    /// Get all error flags from TDES0.
    #[inline(always)]
    #[must_use]
    pub fn error_flags(&self) -> u32 {
        self.tdes0.get() & tdes0::ALL_ERRORS
    }

    /// Get collision count for half-duplex mode.
    #[inline(always)]
    #[must_use]
    pub fn collision_count(&self) -> u8 {
        ((self.tdes0.get() & tdes0::COLLISION_COUNT_MASK) >> tdes0::COLLISION_COUNT_SHIFT)
            as u8
    }

    /// Check if timestamp was captured.
    #[inline(always)]
    #[must_use]
    pub fn has_timestamp(&self) -> bool {
        (self.tdes0.get() & tdes0::TX_TIMESTAMP_STATUS) != 0
    }

    /// Get captured timestamp low 32 bits.
    #[inline(always)]
    #[must_use]
    pub fn timestamp_low(&self) -> u32 {
        self.timestamp_low.get()
    }

    /// Get captured timestamp high 32 bits.
    #[inline(always)]
    #[must_use]
    pub fn timestamp_high(&self) -> u32 {
        self.timestamp_high.get()
    }

    /// Get full 64-bit timestamp.
    #[inline(always)]
    #[must_use]
    pub fn timestamp(&self) -> u64 {
        ((self.timestamp_high.get() as u64) << 32) | (self.timestamp_low.get() as u64)
    }

    /// Get buffer address.
    #[inline(always)]
    #[must_use]
    pub fn buffer_addr(&self) -> u32 {
        self.buffer1_addr.get()
    }

    /// Get next descriptor address in chained mode.
    #[inline(always)]
    #[must_use]
    pub fn next_desc_addr(&self) -> u32 {
        self.buffer2_next_desc.get()
    }

    /// Reset descriptor to initial state.
    pub fn reset(&self) {
        let next = self.buffer2_next_desc.get();
        self.tdes0.set(tdes0::SECOND_ADDR_CHAINED);
        self.tdes1.set(0);
        self.buffer2_next_desc.set(next);
    }

    /// Get raw TDES0 value for debugging.
    #[inline(always)]
    #[must_use]
    pub fn raw_tdes0(&self) -> u32 {
        self.tdes0.get()
    }

    /// Get raw TDES1 value for debugging.
    #[inline(always)]
    #[must_use]
    pub fn raw_tdes1(&self) -> u32 {
        self.tdes1.get()
    }
}

impl Default for TxDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: TxDescriptor uses volatile cells for all DMA-accessed fields
unsafe impl Sync for TxDescriptor {}
unsafe impl Send for TxDescriptor {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::dma::descriptor::bits::{checksum_mode, tdes0, tdes1};

    #[test]
    fn tx_descriptor_size() {
        // TX descriptor must be exactly 32 bytes for ESP32
        #[cfg(not(feature = "esp32p4"))]
        assert_eq!(core::mem::size_of::<TxDescriptor>(), 32);

        // ESP32-P4 uses 64-byte cache-aligned descriptors
        #[cfg(feature = "esp32p4")]
        assert_eq!(core::mem::size_of::<TxDescriptor>(), 64);
    }

    #[test]
    fn tx_descriptor_alignment() {
        // ESP32 requires 4-byte alignment
        #[cfg(not(feature = "esp32p4"))]
        assert_eq!(core::mem::align_of::<TxDescriptor>(), 4);

        // ESP32-P4 requires 64-byte alignment for cache
        #[cfg(feature = "esp32p4")]
        assert_eq!(core::mem::align_of::<TxDescriptor>(), 64);
    }

    #[test]
    fn tx_descriptor_const_size() {
        // Verify the SIZE constant matches actual size
        assert_eq!(TxDescriptor::SIZE, core::mem::size_of::<TxDescriptor>());
    }

    #[test]
    fn tx_descriptor_new_not_owned() {
        let desc = TxDescriptor::new();
        assert!(
            !desc.is_owned(),
            "New descriptor should not be owned by DMA"
        );
    }

    #[test]
    fn tx_descriptor_set_owned() {
        let desc = TxDescriptor::new();
        desc.set_owned();
        assert!(
            desc.is_owned(),
            "Descriptor should be owned after set_owned()"
        );
    }

    #[test]
    fn tx_descriptor_clear_owned() {
        let desc = TxDescriptor::new();
        desc.set_owned();
        assert!(desc.is_owned());
        desc.clear_owned();
        assert!(
            !desc.is_owned(),
            "Descriptor should not be owned after clear_owned()"
        );
    }

    #[test]
    fn tx_descriptor_own_bit_position() {
        // OWN bit should be bit 31 of TDES0
        let desc = TxDescriptor::new();
        desc.set_owned();
        let raw = desc.raw_tdes0();
        assert_eq!(raw & tdes0::OWN, tdes0::OWN, "OWN bit should be bit 31");
    }

    #[test]
    fn tx_descriptor_prepare_first_segment() {
        let desc = TxDescriptor::new();
        desc.prepare(100, true, false);

        let raw = desc.raw_tdes0();
        assert!(
            raw & tdes0::FIRST_SEGMENT != 0,
            "First segment flag should be set"
        );
        assert!(
            raw & tdes0::LAST_SEGMENT == 0,
            "Last segment flag should not be set"
        );
        assert!(raw & tdes0::OWN == 0, "OWN should not be set by prepare()");
    }

    #[test]
    fn tx_descriptor_prepare_last_segment() {
        let desc = TxDescriptor::new();
        desc.prepare(100, false, true);

        let raw = desc.raw_tdes0();
        assert!(
            raw & tdes0::FIRST_SEGMENT == 0,
            "First segment flag should not be set"
        );
        assert!(
            raw & tdes0::LAST_SEGMENT != 0,
            "Last segment flag should be set"
        );
        assert!(
            raw & tdes0::INTERRUPT_ON_COMPLETE != 0,
            "IOC should be set on last segment"
        );
    }

    #[test]
    fn tx_descriptor_prepare_complete_frame() {
        let desc = TxDescriptor::new();
        desc.prepare(1500, true, true);

        let raw = desc.raw_tdes0();
        assert!(raw & tdes0::FIRST_SEGMENT != 0);
        assert!(raw & tdes0::LAST_SEGMENT != 0);
        assert!(raw & tdes0::INTERRUPT_ON_COMPLETE != 0);
    }

    #[test]
    fn tx_descriptor_frame_length() {
        let desc = TxDescriptor::new();

        // Set various frame lengths and verify via raw_tdes1
        desc.prepare(64, true, true);
        let len = desc.raw_tdes1() & tdes1::BUFFER1_SIZE_MASK;
        assert_eq!(len, 64);

        desc.prepare(1500, true, true);
        let len = desc.raw_tdes1() & tdes1::BUFFER1_SIZE_MASK;
        assert_eq!(len, 1500);

        desc.prepare(1518, true, true);
        let len = desc.raw_tdes1() & tdes1::BUFFER1_SIZE_MASK;
        assert_eq!(len, 1518);
    }

    #[test]
    fn tx_descriptor_prepare_and_submit() {
        let desc = TxDescriptor::new();
        desc.prepare_and_submit(256, true, true);

        assert!(
            desc.is_owned(),
            "Descriptor should be owned after prepare_and_submit()"
        );

        // Verify length via raw_tdes1
        let len = desc.raw_tdes1() & tdes1::BUFFER1_SIZE_MASK;
        assert_eq!(len, 256);

        let raw = desc.raw_tdes0();
        assert!(raw & tdes0::FIRST_SEGMENT != 0);
        assert!(raw & tdes0::LAST_SEGMENT != 0);
    }

    #[test]
    fn tx_descriptor_checksum_mode_disabled() {
        let desc = TxDescriptor::new();
        desc.prepare(100, true, true);
        desc.set_checksum_mode(checksum_mode::DISABLED);

        let raw = desc.raw_tdes0();
        let mode = (raw & tdes0::CHECKSUM_INSERT_MASK) >> tdes0::CHECKSUM_INSERT_SHIFT;
        assert_eq!(mode, 0);
    }

    #[test]
    fn tx_descriptor_checksum_mode_ip_only() {
        let desc = TxDescriptor::new();
        desc.prepare(100, true, true);
        desc.set_checksum_mode(checksum_mode::IP_ONLY);

        let raw = desc.raw_tdes0();
        let mode = (raw & tdes0::CHECKSUM_INSERT_MASK) >> tdes0::CHECKSUM_INSERT_SHIFT;
        assert_eq!(mode, 1);
    }

    #[test]
    fn tx_descriptor_checksum_mode_full() {
        let desc = TxDescriptor::new();
        desc.prepare(100, true, true);
        desc.set_checksum_mode(checksum_mode::FULL);

        let raw = desc.raw_tdes0();
        let mode = (raw & tdes0::CHECKSUM_INSERT_MASK) >> tdes0::CHECKSUM_INSERT_SHIFT;
        assert_eq!(mode, 3);
    }

    #[test]
    fn tx_descriptor_no_errors_initially() {
        let desc = TxDescriptor::new();
        assert!(!desc.has_error());
    }

    #[test]
    fn tx_descriptor_error_detection() {
        let desc = TxDescriptor::new();

        // has_error() only checks the error summary bit
        desc.tdes0.set(tdes0::ERR_SUMMARY);
        assert!(desc.has_error());

        // Clear and verify
        desc.tdes0.set(0);
        assert!(!desc.has_error());

        // With error summary and specific error bits
        desc.tdes0.set(tdes0::ERR_SUMMARY | tdes0::UNDERFLOW_ERR);
        assert!(desc.has_error());

        // Can check error flags directly
        let errors = desc.error_flags();
        assert!(errors & tdes0::UNDERFLOW_ERR != 0);

        // Multiple errors with summary
        desc.tdes0
            .set(tdes0::ERR_SUMMARY | tdes0::LATE_COLLISION | tdes0::UNDERFLOW_ERR);
        assert!(desc.has_error());
        let errors = desc.error_flags();
        assert!(errors & tdes0::LATE_COLLISION != 0);
        assert!(errors & tdes0::UNDERFLOW_ERR != 0);
    }

    #[test]
    fn tx_descriptor_reset() {
        let desc = TxDescriptor::new();

        // Set up state
        desc.prepare_and_submit(1000, true, true);
        desc.set_checksum_mode(checksum_mode::FULL);

        // Store next descriptor address
        let next_addr = 0x1234_5678u32;
        desc.buffer2_next_desc.set(next_addr);

        // Reset
        desc.reset();

        // Should not be owned
        assert!(!desc.is_owned());
        // Frame length should be 0 (check via raw_tdes1)
        let len = desc.raw_tdes1() & tdes1::BUFFER1_SIZE_MASK;
        assert_eq!(len, 0);
        // Next descriptor should be preserved
        assert_eq!(desc.next_desc_addr(), next_addr);
        // Chain flag should still be set
        assert!(desc.raw_tdes0() & tdes0::SECOND_ADDR_CHAINED != 0);
    }
}
