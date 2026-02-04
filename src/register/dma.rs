//! DMA Controller Register Definitions
//!
//! The EMAC DMA controller manages data transfers between the MAC and system memory
//! using descriptor-based scatter-gather DMA.

use super::{
    reg_bit_check_clear, reg_bit_ops, reg_ro, reg_rw,
    read_reg, write_reg, DMA_BASE,
};

// =============================================================================
// Register Offsets
// =============================================================================

/// Bus Mode Register offset
pub const DMABUSMODE_OFFSET: usize = 0x00;
/// TX Poll Demand Register offset
pub const DMATXPOLLDEMAND_OFFSET: usize = 0x04;
/// RX Poll Demand Register offset
pub const DMARXPOLLDEMAND_OFFSET: usize = 0x08;
/// RX Descriptor List Address Register offset
pub const DMARXBASEADDR_OFFSET: usize = 0x0C;
/// TX Descriptor List Address Register offset
pub const DMATXBASEADDR_OFFSET: usize = 0x10;
/// Status Register offset
pub const DMASTATUS_OFFSET: usize = 0x14;
/// Operation Mode Register offset
pub const DMAOPERATION_OFFSET: usize = 0x18;
/// Interrupt Enable Register offset
pub const DMAINTENABLE_OFFSET: usize = 0x1C;
/// Missed Frame and Buffer Overflow Counter Register offset
pub const DMAMISSEDFR_OFFSET: usize = 0x20;
/// Receive Interrupt Watchdog Timer Register offset
pub const DMARXWATCHDOG_OFFSET: usize = 0x24;
/// Current Host TX Descriptor Register offset (read-only)
pub const DMACURTXDESC_OFFSET: usize = 0x48;
/// Current Host RX Descriptor Register offset (read-only)
pub const DMACURRXDESC_OFFSET: usize = 0x4C;
/// Current Host TX Buffer Address Register offset (read-only)
pub const DMACURTXBUFADDR_OFFSET: usize = 0x50;
/// Current Host RX Buffer Address Register offset (read-only)
pub const DMACURRXBUFADDR_OFFSET: usize = 0x54;

// =============================================================================
// Bus Mode Register (DMABUSMODE) Bits
// =============================================================================

/// Software Reset - resets all EMAC logic, cleared automatically
pub const DMABUSMODE_SW_RST: u32 = 1 << 0;
/// DMA Arbitration Scheme: 0 = round-robin, 1 = fixed priority
pub const DMABUSMODE_DMA_ARB: u32 = 1 << 1;
/// Descriptor Skip Length shift (number of dwords to skip between descriptors)
pub const DMABUSMODE_DSL_SHIFT: u32 = 2;
/// Descriptor Skip Length mask
pub const DMABUSMODE_DSL_MASK: u32 = 0x1F << 2;
/// Alternate Descriptor Size (8 dwords instead of 4)
pub const DMABUSMODE_ATDS: u32 = 1 << 7;
/// Programmable Burst Length shift (max beats in one DMA transaction)
pub const DMABUSMODE_PBL_SHIFT: u32 = 8;
/// Programmable Burst Length mask
pub const DMABUSMODE_PBL_MASK: u32 = 0x3F << 8;
/// Fixed Burst: 0 = variable length, 1 = fixed length
pub const DMABUSMODE_FB: u32 = 1 << 16;
/// RX DMA Programmable Burst Length shift (when USP=1)
pub const DMABUSMODE_RPBL_SHIFT: u32 = 17;
/// RX DMA Programmable Burst Length mask
pub const DMABUSMODE_RPBL_MASK: u32 = 0x3F << 17;
/// Use Separate PBL: 1 = use RPBL for RX, PBL for TX
pub const DMABUSMODE_USP: u32 = 1 << 23;
/// PBL x8 Mode: multiplies PBL/RPBL by 8
pub const DMABUSMODE_PBL_X8: u32 = 1 << 24;
/// Address Aligned Beats: burst transfers aligned to start address
pub const DMABUSMODE_AAL: u32 = 1 << 25;
/// Mixed Burst: allows mixing of fixed and undefined bursts
pub const DMABUSMODE_MB: u32 = 1 << 26;
/// Transmit Priority: 0 = round-robin, 1 = TX has priority
pub const DMABUSMODE_TXPR: u32 = 1 << 27;

// =============================================================================
// Status Register (DMASTATUS) Bits
// =============================================================================

/// Transmit Interrupt - frame transmission complete
pub const DMASTATUS_TI: u32 = 1 << 0;
/// Transmit Process Stopped
pub const DMASTATUS_TPS: u32 = 1 << 1;
/// Transmit Buffer Unavailable
pub const DMASTATUS_TU: u32 = 1 << 2;
/// Transmit Jabber Timeout
pub const DMASTATUS_TJT: u32 = 1 << 3;
/// Receive Overflow
pub const DMASTATUS_OVF: u32 = 1 << 4;
/// Transmit Underflow
pub const DMASTATUS_UNF: u32 = 1 << 5;
/// Receive Interrupt - frame reception complete
pub const DMASTATUS_RI: u32 = 1 << 6;
/// Receive Buffer Unavailable
pub const DMASTATUS_RU: u32 = 1 << 7;
/// Receive Process Stopped
pub const DMASTATUS_RPS: u32 = 1 << 8;
/// Receive Watchdog Timeout
pub const DMASTATUS_RWT: u32 = 1 << 9;
/// Early Transmit Interrupt
pub const DMASTATUS_ETI: u32 = 1 << 10;
/// Fatal Bus Error Interrupt
pub const DMASTATUS_FBI: u32 = 1 << 13;
/// Early Receive Interrupt
pub const DMASTATUS_ERI: u32 = 1 << 14;
/// Abnormal Interrupt Summary
pub const DMASTATUS_AIS: u32 = 1 << 15;
/// Normal Interrupt Summary
pub const DMASTATUS_NIS: u32 = 1 << 16;
/// Receive Process State shift
pub const DMASTATUS_RS_SHIFT: u32 = 17;
/// Receive Process State mask
pub const DMASTATUS_RS_MASK: u32 = 0x7 << 17;
/// Transmit Process State shift
pub const DMASTATUS_TS_SHIFT: u32 = 20;
/// Transmit Process State mask
pub const DMASTATUS_TS_MASK: u32 = 0x7 << 20;
/// Error Bits shift (type of bus error)
pub const DMASTATUS_EB_SHIFT: u32 = 23;
/// Error Bits mask
pub const DMASTATUS_EB_MASK: u32 = 0x7 << 23;

/// All interrupt status bits (for clearing)
pub const DMASTATUS_ALL_INTERRUPTS: u32 = DMASTATUS_TI
    | DMASTATUS_TPS
    | DMASTATUS_TU
    | DMASTATUS_TJT
    | DMASTATUS_OVF
    | DMASTATUS_UNF
    | DMASTATUS_RI
    | DMASTATUS_RU
    | DMASTATUS_RPS
    | DMASTATUS_RWT
    | DMASTATUS_ETI
    | DMASTATUS_FBI
    | DMASTATUS_ERI
    | DMASTATUS_AIS
    | DMASTATUS_NIS;

// =============================================================================
// Operation Mode Register (DMAOPERATION) Bits
// =============================================================================

/// Start/Stop Receive: 1 = start DMA receive
pub const DMAOPERATION_SR: u32 = 1 << 1;
/// Operate on Second Frame: start TX before status of first frame
pub const DMAOPERATION_OSF: u32 = 1 << 2;
/// Receive Threshold Control shift
pub const DMAOPERATION_RTC_SHIFT: u32 = 3;
/// Receive Threshold Control mask
pub const DMAOPERATION_RTC_MASK: u32 = 0x3 << 3;
/// Forward Undersized Good Frames
pub const DMAOPERATION_FUF: u32 = 1 << 6;
/// Forward Error Frames
pub const DMAOPERATION_FEF: u32 = 1 << 7;
/// Start/Stop Transmission: 1 = start DMA transmit
pub const DMAOPERATION_ST: u32 = 1 << 13;
/// Transmit Threshold Control shift
pub const DMAOPERATION_TTC_SHIFT: u32 = 14;
/// Transmit Threshold Control mask
pub const DMAOPERATION_TTC_MASK: u32 = 0x7 << 14;
/// Flush Transmit FIFO
pub const DMAOPERATION_FTF: u32 = 1 << 20;
/// Transmit Store and Forward
pub const DMAOPERATION_TSF: u32 = 1 << 21;
/// Disable Flushing of Received Frames
pub const DMAOPERATION_DFF: u32 = 1 << 24;
/// Receive Store and Forward
pub const DMAOPERATION_RSF: u32 = 1 << 25;
/// Disable Dropping of TCP/IP Checksum Error Frames
pub const DMAOPERATION_DT: u32 = 1 << 26;

/// RTC threshold values (when to transfer RX data to memory)
pub mod rtc {
    /// Transfer when 64 bytes received
    pub const RTC_64: u32 = 0;
    /// Transfer when 32 bytes received
    pub const RTC_32: u32 = 1;
    /// Transfer when 96 bytes received
    pub const RTC_96: u32 = 2;
    /// Transfer when 128 bytes received
    pub const RTC_128: u32 = 3;
}

/// TTC threshold values (when to start TX DMA)
pub mod ttc {
    /// Start when 64 bytes in TX FIFO
    pub const TTC_64: u32 = 0;
    /// Start when 128 bytes in TX FIFO
    pub const TTC_128: u32 = 1;
    /// Start when 192 bytes in TX FIFO
    pub const TTC_192: u32 = 2;
    /// Start when 256 bytes in TX FIFO
    pub const TTC_256: u32 = 3;
    /// Start when 40 bytes in TX FIFO
    pub const TTC_40: u32 = 4;
    /// Start when 32 bytes in TX FIFO
    pub const TTC_32: u32 = 5;
    /// Start when 24 bytes in TX FIFO
    pub const TTC_24: u32 = 6;
    /// Start when 16 bytes in TX FIFO
    pub const TTC_16: u32 = 7;
}

// =============================================================================
// Interrupt Enable Register (DMAINTENABLE) Bits
// =============================================================================

/// Transmit Interrupt Enable
pub const DMAINTEN_TIE: u32 = 1 << 0;
/// Transmit Stopped Enable
pub const DMAINTEN_TSE: u32 = 1 << 1;
/// Transmit Buffer Unavailable Enable
pub const DMAINTEN_TUE: u32 = 1 << 2;
/// Transmit Jabber Timeout Enable
pub const DMAINTEN_TJE: u32 = 1 << 3;
/// Overflow Interrupt Enable
pub const DMAINTEN_OVE: u32 = 1 << 4;
/// Underflow Interrupt Enable
pub const DMAINTEN_UNE: u32 = 1 << 5;
/// Receive Interrupt Enable
pub const DMAINTEN_RIE: u32 = 1 << 6;
/// Receive Buffer Unavailable Enable
pub const DMAINTEN_RUE: u32 = 1 << 7;
/// Receive Stopped Enable
pub const DMAINTEN_RSE: u32 = 1 << 8;
/// Receive Watchdog Timeout Enable
pub const DMAINTEN_RWE: u32 = 1 << 9;
/// Early Transmit Interrupt Enable
pub const DMAINTEN_ETE: u32 = 1 << 10;
/// Fatal Bus Error Enable
pub const DMAINTEN_FBE: u32 = 1 << 13;
/// Early Receive Interrupt Enable
pub const DMAINTEN_ERE: u32 = 1 << 14;
/// Abnormal Interrupt Summary Enable
pub const DMAINTEN_AIE: u32 = 1 << 15;
/// Normal Interrupt Summary Enable
pub const DMAINTEN_NIE: u32 = 1 << 16;

/// Default interrupt enable mask (normal operation)
pub const DMAINTEN_DEFAULT: u32 =
    DMAINTEN_TIE | DMAINTEN_RIE | DMAINTEN_FBE | DMAINTEN_AIE | DMAINTEN_NIE;

// =============================================================================
// DMA Register Access Functions
// =============================================================================

/// DMA Register block for type-safe access
pub struct DmaRegs;

impl DmaRegs {
    /// Get the base address
    #[inline(always)]
    pub const fn base() -> usize {
        DMA_BASE
    }

    // -------------------------------------------------------------------------
    // Register accessors (generated by macros)
    // -------------------------------------------------------------------------
    
    reg_rw!(bus_mode, set_bus_mode, DMA_BASE, DMABUSMODE_OFFSET, "Bus Mode register");
    reg_rw!(status, set_status, DMA_BASE, DMASTATUS_OFFSET, "Status register");
    reg_rw!(operation_mode, set_operation_mode, DMA_BASE, DMAOPERATION_OFFSET, "Operation Mode register");
    reg_rw!(interrupt_enable, set_interrupt_enable, DMA_BASE, DMAINTENABLE_OFFSET, "Interrupt Enable register");
    
    reg_ro!(missed_frames, DMA_BASE, DMAMISSEDFR_OFFSET, "Missed Frame counter");
    reg_ro!(current_tx_desc, DMA_BASE, DMACURTXDESC_OFFSET, "Current TX Descriptor address");
    reg_ro!(current_rx_desc, DMA_BASE, DMACURRXDESC_OFFSET, "Current RX Descriptor address");
    reg_ro!(current_tx_buffer, DMA_BASE, DMACURTXBUFADDR_OFFSET, "Current TX Buffer address");
    reg_ro!(current_rx_buffer, DMA_BASE, DMACURRXBUFADDR_OFFSET, "Current RX Buffer address");

    // -------------------------------------------------------------------------
    // Bit operations (generated by macros)
    // -------------------------------------------------------------------------
    
    reg_bit_ops!(start_tx, stop_tx, DMA_BASE, DMAOPERATION_OFFSET, DMAOPERATION_ST, "TX DMA", "Start", "Stop");
    reg_bit_ops!(start_rx, stop_rx, DMA_BASE, DMAOPERATION_OFFSET, DMAOPERATION_SR, "RX DMA", "Start", "Stop");
    
    reg_bit_check_clear!(is_tx_fifo_flush_complete, DMA_BASE, DMAOPERATION_OFFSET, DMAOPERATION_FTF, 
                         "Check if TX FIFO flush is complete");
    reg_bit_check_clear!(is_reset_complete, DMA_BASE, DMABUSMODE_OFFSET, DMABUSMODE_SW_RST,
                         "Check if software reset is complete");

    // -------------------------------------------------------------------------
    // Special operations (cannot be generated by simple macros)
    // -------------------------------------------------------------------------

    /// Issue TX poll demand (wake up TX DMA)
    #[inline(always)]
    pub fn tx_poll_demand() {
        unsafe { write_reg(DMA_BASE + DMATXPOLLDEMAND_OFFSET, 0) }
    }

    /// Issue RX poll demand (wake up RX DMA)
    #[inline(always)]
    pub fn rx_poll_demand() {
        unsafe { write_reg(DMA_BASE + DMARXPOLLDEMAND_OFFSET, 0) }
    }

    /// Set RX descriptor list base address
    #[inline(always)]
    pub fn set_rx_desc_list_addr(addr: u32) {
        unsafe { write_reg(DMA_BASE + DMARXBASEADDR_OFFSET, addr) }
    }

    /// Set TX descriptor list base address
    #[inline(always)]
    pub fn set_tx_desc_list_addr(addr: u32) {
        unsafe { write_reg(DMA_BASE + DMATXBASEADDR_OFFSET, addr) }
    }

    /// Clear all interrupt status flags
    #[inline(always)]
    pub fn clear_all_interrupts() {
        Self::set_status(DMASTATUS_ALL_INTERRUPTS);
    }

    /// Flush TX FIFO
    #[inline(always)]
    pub fn flush_tx_fifo() {
        unsafe {
            let mode = read_reg(DMA_BASE + DMAOPERATION_OFFSET);
            write_reg(DMA_BASE + DMAOPERATION_OFFSET, mode | DMAOPERATION_FTF);
        }
    }

    /// Enable default interrupts
    #[inline(always)]
    pub fn enable_default_interrupts() {
        Self::set_interrupt_enable(DMAINTEN_DEFAULT);
    }

    /// Disable all interrupts
    #[inline(always)]
    pub fn disable_all_interrupts() {
        Self::set_interrupt_enable(0);
    }

    /// Set RX interrupt watchdog timer
    #[inline(always)]
    pub fn set_rx_watchdog(value: u8) {
        unsafe { write_reg(DMA_BASE + DMARXWATCHDOG_OFFSET, value as u32) }
    }

    /// Initiate software reset
    #[inline(always)]
    pub fn software_reset() {
        Self::set_bus_mode(DMABUSMODE_SW_RST);
    }
}

/// Receive process states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RxProcessState {
    /// Stopped: Reset or Stop Receive Command issued
    Stopped = 0,
    /// Running: Fetching Receive Transfer Descriptor
    FetchingDescriptor = 1,
    /// Reserved
    Reserved2 = 2,
    /// Running: Waiting for receive packet
    WaitingForPacket = 3,
    /// Suspended: Receive Descriptor Unavailable
    Suspended = 4,
    /// Running: Closing Receive Descriptor
    ClosingDescriptor = 5,
    /// Reserved
    Reserved6 = 6,
    /// Running: Transferring data to host memory
    TransferringData = 7,
}

impl From<u32> for RxProcessState {
    fn from(value: u32) -> Self {
        match (value & DMASTATUS_RS_MASK) >> DMASTATUS_RS_SHIFT {
            0 => RxProcessState::Stopped,
            1 => RxProcessState::FetchingDescriptor,
            2 => RxProcessState::Reserved2,
            3 => RxProcessState::WaitingForPacket,
            4 => RxProcessState::Suspended,
            5 => RxProcessState::ClosingDescriptor,
            6 => RxProcessState::Reserved6,
            _ => RxProcessState::TransferringData,
        }
    }
}

/// Transmit process states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TxProcessState {
    /// Stopped: Reset or Stop Transmit Command issued
    Stopped = 0,
    /// Running: Fetching Transmit Transfer Descriptor
    FetchingDescriptor = 1,
    /// Running: Waiting for status
    WaitingForStatus = 2,
    /// Running: Reading Data from host memory
    ReadingData = 3,
    /// Reserved
    Reserved4 = 4,
    /// Reserved
    Reserved5 = 5,
    /// Suspended: Transmit Descriptor Unavailable
    Suspended = 6,
    /// Running: Closing Transmit Descriptor
    ClosingDescriptor = 7,
}

impl From<u32> for TxProcessState {
    fn from(value: u32) -> Self {
        match (value & DMASTATUS_TS_MASK) >> DMASTATUS_TS_SHIFT {
            0 => TxProcessState::Stopped,
            1 => TxProcessState::FetchingDescriptor,
            2 => TxProcessState::WaitingForStatus,
            3 => TxProcessState::ReadingData,
            4 => TxProcessState::Reserved4,
            5 => TxProcessState::Reserved5,
            6 => TxProcessState::Suspended,
            _ => TxProcessState::ClosingDescriptor,
        }
    }
}
