# ESP32 EMAC Rust Implementation Design Document

## Overview

This document describes the design for a `no_std`, `no_alloc` Rust implementation of the ESP32 Ethernet MAC (EMAC) controller. The implementation targets bare-metal embedded environments without heap allocation, using only static memory.

## Goals

- **`no_std`**: No standard library dependency
- **`no_alloc`**: Zero heap allocations - all memory statically allocated at compile time
- **Safe abstractions**: Leverage Rust's type system for correctness
- **Zero-cost abstractions**: No runtime overhead compared to C implementation
- **Portable**: Support ESP32 and ESP32-P4 variants via feature flags

## Hardware Background

The ESP32 EMAC is based on the Synopsys DesignWare MAC (DWMAC) IP core featuring:

- IEEE 802.3 compliant MAC
- MII/RMII interface support
- DMA engine with descriptor-based transfers
- MDIO interface for PHY management
- 10/100 Mbps operation
- Full/half duplex support
- Hardware checksum offload (optional)
- IEEE 1588 PTP timestamps (ESP32-P4)

### Memory Map

| Peripheral | Base Address (ESP32) |
|------------|---------------------|
| EMAC DMA   | 0x3FF69000          |
| EMAC MAC   | 0x3FF6A000          |
| EMAC EXT   | 0x3FF69800          |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                        │
│                  (smoltcp / embedded-nal / raw)                 │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                          EmacDriver                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  MacEngine  │  │  DmaEngine  │  │     PhyInterface        │  │
│  │             │  │             │  │  (MDIO read/write)      │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Hardware Abstraction Layer                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐ │
│  │ Registers│  │  Clock   │  │   GPIO   │  │  Descriptors     │ │
│  │ (DMA/MAC)│  │  Config  │  │  Config  │  │  (TX/RX rings)   │ │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                         ESP32 Hardware                          │
│          EMAC DMA  │  EMAC MAC  │  EMAC EXT  │  PHY (external)  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Module Structure

```
src/
├── lib.rs                 # Crate root, public API exports
├── error.rs               # Error types (no heap, enum-based)
├── config.rs              # Configuration structures
│
├── register/              # Memory-mapped register definitions
│   ├── mod.rs
│   ├── dma.rs             # DMA controller registers
│   ├── mac.rs             # MAC core registers
│   └── ext.rs             # Clock/GPIO extension registers
│
├── descriptor/            # DMA descriptor definitions
│   ├── mod.rs
│   ├── tx.rs              # Transmit descriptors
│   └── rx.rs              # Receive descriptors
│
├── hal/                   # Hardware abstraction
│   ├── mod.rs
│   ├── clock.rs           # Clock tree configuration
│   ├── gpio.rs            # Pin multiplexing
│   ├── mdio.rs            # PHY register access
│   └── reset.rs           # Reset controller
│
├── dma.rs                 # DMA engine (buffer management)
├── mac.rs                 # MAC driver implementation
└── traits.rs              # Public traits for extensibility
```

---

## Memory Model (No Alloc)

All memory is statically allocated using const generics for configurability.

### Static Buffer Allocation

```rust
/// Compile-time configured EMAC instance
/// RX_BUFS: Number of receive buffers (typically 10)
/// TX_BUFS: Number of transmit buffers (typically 10)
/// BUF_SIZE: Size of each buffer in bytes (typically 1600)
pub struct Emac<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    // All fields are inline, no heap allocation
    rx_descriptors: [RxDescriptor; RX_BUFS],
    tx_descriptors: [TxDescriptor; TX_BUFS],
    rx_buffers: [[u8; BUF_SIZE]; RX_BUFS],
    tx_buffers: [[u8; BUF_SIZE]; TX_BUFS],
    state: EmacState,
    // ... other inline fields
}
```

### Memory Layout Requirements

| Component | Size (ESP32) | Alignment | Location |
|-----------|-------------|-----------|----------|
| RX Descriptor | 32 bytes | 4 bytes | DMA-capable SRAM |
| TX Descriptor | 32 bytes | 4 bytes | DMA-capable SRAM |
| RX Descriptor (P4) | 64 bytes | 64 bytes | DMA-capable SRAM |
| TX Descriptor (P4) | 64 bytes | 64 bytes | DMA-capable SRAM |
| DMA Buffer | 1600 bytes | 4 bytes | DMA-capable SRAM |

### Link Section Placement

```rust
/// Place DMA structures in appropriate memory section
#[link_section = ".dram1.emac"]
static mut EMAC_INSTANCE: MaybeUninit<Emac<10, 10, 1600>> = MaybeUninit::uninit();
```

### Memory Budget (Default Configuration)

```
RX Descriptors: 10 × 32 bytes  =    320 bytes
TX Descriptors: 10 × 32 bytes  =    320 bytes
RX Buffers:     10 × 1600      = 16,000 bytes
TX Buffers:     10 × 1600      = 16,000 bytes
Driver State:                  ≈    128 bytes
─────────────────────────────────────────────
Total:                         ≈ 32,768 bytes (32 KB)
```

---

## Register Definitions

### Design Principles

1. **Volatile access**: All register access through volatile read/write
2. **Type safety**: Bitfields represented as typed structs
3. **Zero-cost**: Inline functions compile to direct memory access

### DMA Registers

```rust
/// DMA register block base address
pub const DMA_BASE: usize = 0x3FF6_9000;

/// DMA Bus Mode Register (offset 0x00)
#[repr(transparent)]
pub struct DmaBusMode(u32);

impl DmaBusMode {
    pub const SW_RST: u32 = 1 << 0;
    pub const DMA_ARB_SCH: u32 = 1 << 1;
    pub const DESC_SKIP_LEN_SHIFT: u32 = 2;
    pub const DESC_SKIP_LEN_MASK: u32 = 0x1F << 2;
    pub const ALT_DESC_SIZE: u32 = 1 << 7;
    pub const PROG_BURST_LEN_SHIFT: u32 = 8;
    pub const PROG_BURST_LEN_MASK: u32 = 0x3F << 8;
    pub const FIXED_BURST: u32 = 1 << 16;
    pub const RX_DMA_PBL_SHIFT: u32 = 17;
    pub const RX_DMA_PBL_MASK: u32 = 0x3F << 17;
    pub const USE_SEP_PBL: u32 = 1 << 23;
    pub const PBL_X8_MODE: u32 = 1 << 24;
    pub const ADDR_ALIGN_BEATS: u32 = 1 << 25;
    pub const MIXED_BURST: u32 = 1 << 26;
    
    #[inline(always)]
    pub fn read() -> Self {
        Self(unsafe { core::ptr::read_volatile((DMA_BASE + 0x00) as *const u32) })
    }
    
    #[inline(always)]
    pub fn write(self) {
        unsafe { core::ptr::write_volatile((DMA_BASE + 0x00) as *mut u32, self.0) }
    }
    
    #[inline(always)]
    pub fn software_reset(&self) -> bool {
        (self.0 & Self::SW_RST) != 0
    }
}

/// DMA Status Register (offset 0x14)
#[repr(transparent)]
pub struct DmaStatus(u32);

impl DmaStatus {
    pub const TX_INT: u32 = 1 << 0;
    pub const TX_STOPPED: u32 = 1 << 1;
    pub const TX_BUF_UNAVAIL: u32 = 1 << 2;
    pub const TX_JABBER_TIMEOUT: u32 = 1 << 3;
    pub const RX_OVERFLOW: u32 = 1 << 4;
    pub const TX_UNDERFLOW: u32 = 1 << 5;
    pub const RX_INT: u32 = 1 << 6;
    pub const RX_BUF_UNAVAIL: u32 = 1 << 7;
    pub const RX_STOPPED: u32 = 1 << 8;
    pub const RX_WATCHDOG_TIMEOUT: u32 = 1 << 9;
    pub const TX_EARLY_INT: u32 = 1 << 10;
    pub const FATAL_BUS_ERR: u32 = 1 << 13;
    pub const RX_EARLY_INT: u32 = 1 << 14;
    pub const ABNORMAL_INT_SUMMARY: u32 = 1 << 15;
    pub const NORMAL_INT_SUMMARY: u32 = 1 << 16;
    // ... state fields
}

/// DMA Operation Mode Register (offset 0x18)
#[repr(transparent)]
pub struct DmaOperationMode(u32);

impl DmaOperationMode {
    pub const START_RX: u32 = 1 << 1;
    pub const OPT_SECOND_FRAME: u32 = 1 << 2;
    pub const RX_THRESH_CTRL_SHIFT: u32 = 3;
    pub const FWD_UNDERSIZED_FRAMES: u32 = 1 << 6;
    pub const FWD_ERROR_FRAMES: u32 = 1 << 7;
    pub const START_TX: u32 = 1 << 13;
    pub const TX_THRESH_CTRL_SHIFT: u32 = 14;
    pub const FLUSH_TX_FIFO: u32 = 1 << 20;
    pub const TX_STORE_FWD: u32 = 1 << 21;
    pub const RX_STORE_FWD: u32 = 1 << 25;
    // ...
}
```

### MAC Registers

```rust
/// MAC register block base address
pub const MAC_BASE: usize = 0x3FF6_A000;

/// GMAC Configuration Register (offset 0x00)
#[repr(transparent)]
pub struct GmacConfig(u32);

impl GmacConfig {
    pub const PREAMBLE_LEN_SHIFT: u32 = 0;
    pub const RX_ENABLE: u32 = 1 << 2;
    pub const TX_ENABLE: u32 = 1 << 3;
    pub const DEFERRAL_CHECK: u32 = 1 << 4;
    pub const BACK_OFF_LIMIT_SHIFT: u32 = 5;
    pub const AUTO_PAD_CRC_STRIP: u32 = 1 << 7;
    pub const RETRY_DISABLE: u32 = 1 << 9;
    pub const RX_IPC_OFFLOAD: u32 = 1 << 10;
    pub const DUPLEX_MODE: u32 = 1 << 11;
    pub const LOOPBACK: u32 = 1 << 12;
    pub const SPEED_100M: u32 = 1 << 14;
    pub const PORT_MII: u32 = 1 << 15;
    pub const CARRIER_SENSE_DISABLE: u32 = 1 << 16;
    pub const INTER_FRAME_GAP_SHIFT: u32 = 17;
    pub const JABBER_DISABLE: u32 = 1 << 22;
    pub const WATCHDOG_DISABLE: u32 = 1 << 23;
    pub const CRC_STRIP_TYPE_FRAMES: u32 = 1 << 25;
    // ...
}

/// GMAC Frame Filter Register (offset 0x04)
#[repr(transparent)]
pub struct GmacFrameFilter(u32);

impl GmacFrameFilter {
    pub const PROMISCUOUS: u32 = 1 << 0;
    pub const HASH_UNICAST: u32 = 1 << 1;
    pub const HASH_MULTICAST: u32 = 1 << 2;
    pub const DA_INVERSE_FILTER: u32 = 1 << 3;
    pub const PASS_ALL_MULTICAST: u32 = 1 << 4;
    pub const DISABLE_BROADCAST: u32 = 1 << 5;
    pub const PASS_CTRL_FRAMES_SHIFT: u32 = 6;
    pub const SA_INVERSE_FILTER: u32 = 1 << 8;
    pub const SA_FILTER_ENABLE: u32 = 1 << 9;
    pub const HASH_PERFECT_FILTER: u32 = 1 << 10;
    pub const RECEIVE_ALL: u32 = 1 << 31;
}
```

---

## DMA Descriptors

### Transmit Descriptor

```rust
/// Transmit DMA Descriptor
/// Size: 32 bytes (ESP32) or 64 bytes (ESP32-P4 with cache alignment)
#[repr(C, align(4))]
pub struct TxDescriptor {
    /// TDES0: Status and control bits
    tdes0: VolatileCell<u32>,
    /// TDES1: Buffer sizes
    tdes1: VolatileCell<u32>,
    /// TDES2: Buffer 1 address
    buffer1_addr: VolatileCell<u32>,
    /// TDES3: Buffer 2 address or next descriptor address
    buffer2_next_desc_addr: VolatileCell<u32>,
    /// Reserved / Extended status
    _reserved1: u32,
    _reserved2: u32,
    /// Timestamp low (when enabled)
    timestamp_low: VolatileCell<u32>,
    /// Timestamp high (when enabled)
    timestamp_high: VolatileCell<u32>,
}

/// TDES0 bit definitions
pub mod tdes0 {
    pub const DEFERRED: u32 = 1 << 0;
    pub const UNDERFLOW_ERR: u32 = 1 << 1;
    pub const EXCESSIVE_DEFERRAL: u32 = 1 << 2;
    pub const COLLISION_COUNT_SHIFT: u32 = 3;
    pub const COLLISION_COUNT_MASK: u32 = 0xF << 3;
    pub const VLAN_FRAME: u32 = 1 << 7;
    pub const EXCESSIVE_COLLISION: u32 = 1 << 8;
    pub const LATE_COLLISION: u32 = 1 << 9;
    pub const NO_CARRIER: u32 = 1 << 10;
    pub const LOSS_OF_CARRIER: u32 = 1 << 11;
    pub const IP_PAYLOAD_ERR: u32 = 1 << 12;
    pub const FRAME_FLUSHED: u32 = 1 << 13;
    pub const JABBER_TIMEOUT: u32 = 1 << 14;
    pub const ERR_SUMMARY: u32 = 1 << 15;
    pub const IP_HEADER_ERR: u32 = 1 << 16;
    pub const TX_TIMESTAMP_STATUS: u32 = 1 << 17;
    pub const VLAN_INSERT_CTRL_SHIFT: u32 = 18;
    pub const SECOND_ADDR_CHAINED: u32 = 1 << 20;
    pub const TX_END_OF_RING: u32 = 1 << 21;
    pub const CHECKSUM_INSERT_CTRL_SHIFT: u32 = 22;
    pub const CRC_REPLACE_CTRL: u32 = 1 << 24;
    pub const TX_TIMESTAMP_ENABLE: u32 = 1 << 25;
    pub const DISABLE_PAD: u32 = 1 << 26;
    pub const DISABLE_CRC: u32 = 1 << 27;
    pub const FIRST_SEGMENT: u32 = 1 << 28;
    pub const LAST_SEGMENT: u32 = 1 << 29;
    pub const INTERRUPT_ON_COMPLETE: u32 = 1 << 30;
    pub const OWN: u32 = 1 << 31;
}

/// TDES1 bit definitions
pub mod tdes1 {
    pub const TX_BUFFER1_SIZE_MASK: u32 = 0x1FFF;
    pub const TX_BUFFER1_SIZE_SHIFT: u32 = 0;
    pub const TX_BUFFER2_SIZE_MASK: u32 = 0x1FFF << 16;
    pub const TX_BUFFER2_SIZE_SHIFT: u32 = 16;
    pub const SA_INSERT_CTRL_SHIFT: u32 = 29;
}

impl TxDescriptor {
    /// Create a new uninitialized descriptor
    pub const fn new() -> Self {
        Self {
            tdes0: VolatileCell::new(0),
            tdes1: VolatileCell::new(0),
            buffer1_addr: VolatileCell::new(0),
            buffer2_next_desc_addr: VolatileCell::new(0),
            _reserved1: 0,
            _reserved2: 0,
            timestamp_low: VolatileCell::new(0),
            timestamp_high: VolatileCell::new(0),
        }
    }
    
    /// Check if descriptor is owned by DMA
    #[inline(always)]
    pub fn is_owned_by_dma(&self) -> bool {
        (self.tdes0.get() & tdes0::OWN) != 0
    }
    
    /// Give descriptor ownership to DMA
    #[inline(always)]
    pub fn set_owned_by_dma(&self) {
        self.tdes0.set(self.tdes0.get() | tdes0::OWN);
    }
    
    /// Configure for single-buffer chained mode
    pub fn setup_chained(&self, buffer: *const u8, next_desc: *const TxDescriptor) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc_addr.set(next_desc as u32);
        self.tdes0.set(tdes0::SECOND_ADDR_CHAINED);
    }
    
    /// Set buffer size and control flags for transmission
    pub fn prepare_tx(&self, len: usize, first: bool, last: bool) {
        let mut flags = tdes0::SECOND_ADDR_CHAINED;
        if first {
            flags |= tdes0::FIRST_SEGMENT;
        }
        if last {
            flags |= tdes0::LAST_SEGMENT | tdes0::INTERRUPT_ON_COMPLETE;
        }
        
        self.tdes1.set((len as u32) & tdes1::TX_BUFFER1_SIZE_MASK);
        self.tdes0.set(flags);
    }
}
```

### Receive Descriptor

```rust
/// Receive DMA Descriptor
#[repr(C, align(4))]
pub struct RxDescriptor {
    /// RDES0: Status bits
    rdes0: VolatileCell<u32>,
    /// RDES1: Control and buffer sizes
    rdes1: VolatileCell<u32>,
    /// RDES2: Buffer 1 address
    buffer1_addr: VolatileCell<u32>,
    /// RDES3: Buffer 2 address or next descriptor address
    buffer2_next_desc_addr: VolatileCell<u32>,
    /// Extended status
    extended_status: VolatileCell<u32>,
    _reserved: u32,
    /// Timestamp low
    timestamp_low: VolatileCell<u32>,
    /// Timestamp high
    timestamp_high: VolatileCell<u32>,
}

/// RDES0 bit definitions
pub mod rdes0 {
    pub const EXTENDED_STATUS_AVAIL: u32 = 1 << 0;
    pub const CRC_ERR: u32 = 1 << 1;
    pub const DRIBBLE_BIT_ERR: u32 = 1 << 2;
    pub const RX_ERR: u32 = 1 << 3;
    pub const RX_WATCHDOG_TIMEOUT: u32 = 1 << 4;
    pub const FRAME_TYPE: u32 = 1 << 5;
    pub const LATE_COLLISION: u32 = 1 << 6;
    pub const TIMESTAMP_AVAIL: u32 = 1 << 7;
    pub const LAST_DESCRIPTOR: u32 = 1 << 8;
    pub const FIRST_DESCRIPTOR: u32 = 1 << 9;
    pub const VLAN_TAG: u32 = 1 << 10;
    pub const OVERFLOW_ERR: u32 = 1 << 11;
    pub const LENGTH_ERR: u32 = 1 << 12;
    pub const SA_FILTER_FAIL: u32 = 1 << 13;
    pub const DESCRIPTOR_ERR: u32 = 1 << 14;
    pub const ERR_SUMMARY: u32 = 1 << 15;
    pub const FRAME_LENGTH_SHIFT: u32 = 16;
    pub const FRAME_LENGTH_MASK: u32 = 0x3FFF << 16;
    pub const DA_FILTER_FAIL: u32 = 1 << 30;
    pub const OWN: u32 = 1 << 31;
}

/// RDES1 bit definitions
pub mod rdes1 {
    pub const RX_BUFFER1_SIZE_MASK: u32 = 0x1FFF;
    pub const RX_BUFFER1_SIZE_SHIFT: u32 = 0;
    pub const SECOND_ADDR_CHAINED: u32 = 1 << 14;
    pub const RX_END_OF_RING: u32 = 1 << 15;
    pub const RX_BUFFER2_SIZE_MASK: u32 = 0x1FFF << 16;
    pub const RX_BUFFER2_SIZE_SHIFT: u32 = 16;
    pub const DISABLE_INT_ON_COMPLETE: u32 = 1 << 31;
}

impl RxDescriptor {
    pub const fn new() -> Self {
        Self {
            rdes0: VolatileCell::new(0),
            rdes1: VolatileCell::new(0),
            buffer1_addr: VolatileCell::new(0),
            buffer2_next_desc_addr: VolatileCell::new(0),
            extended_status: VolatileCell::new(0),
            _reserved: 0,
            timestamp_low: VolatileCell::new(0),
            timestamp_high: VolatileCell::new(0),
        }
    }
    
    /// Check if descriptor is owned by DMA
    #[inline(always)]
    pub fn is_owned_by_dma(&self) -> bool {
        (self.rdes0.get() & rdes0::OWN) != 0
    }
    
    /// Give descriptor back to DMA
    #[inline(always)]
    pub fn set_owned_by_dma(&self) {
        self.rdes0.set(rdes0::OWN);
    }
    
    /// Get received frame length (excluding CRC)
    #[inline(always)]
    pub fn frame_length(&self) -> usize {
        ((self.rdes0.get() & rdes0::FRAME_LENGTH_MASK) >> rdes0::FRAME_LENGTH_SHIFT) as usize
    }
    
    /// Check if this is the first descriptor of a frame
    #[inline(always)]
    pub fn is_first_descriptor(&self) -> bool {
        (self.rdes0.get() & rdes0::FIRST_DESCRIPTOR) != 0
    }
    
    /// Check if this is the last descriptor of a frame
    #[inline(always)]
    pub fn is_last_descriptor(&self) -> bool {
        (self.rdes0.get() & rdes0::LAST_DESCRIPTOR) != 0
    }
    
    /// Check for any receive errors
    #[inline(always)]
    pub fn has_error(&self) -> bool {
        (self.rdes0.get() & rdes0::ERR_SUMMARY) != 0
    }
    
    /// Configure for chained mode with buffer
    pub fn setup_chained(&self, buffer: *mut u8, buffer_size: usize, next_desc: *const RxDescriptor) {
        self.buffer1_addr.set(buffer as u32);
        self.buffer2_next_desc_addr.set(next_desc as u32);
        self.rdes1.set(
            rdes1::SECOND_ADDR_CHAINED | 
            ((buffer_size as u32) & rdes1::RX_BUFFER1_SIZE_MASK)
        );
        self.rdes0.set(rdes0::OWN);  // Give to DMA
    }
}
```

---

## DMA Engine

### Design

The DMA engine manages descriptor rings and buffer transfers without heap allocation.

```rust
/// DMA Engine with static allocation
pub struct DmaEngine<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    /// RX descriptor ring (circular)
    rx_ring: DescriptorRing<RxDescriptor, RX_BUFS>,
    /// TX descriptor ring (circular)
    tx_ring: DescriptorRing<TxDescriptor, TX_BUFS>,
    /// RX data buffers
    rx_buffers: [[u8; BUF_SIZE]; RX_BUFS],
    /// TX data buffers
    tx_buffers: [[u8; BUF_SIZE]; TX_BUFS],
    /// TX descriptor control flags
    tx_flags: u32,
}

/// Circular descriptor ring
pub struct DescriptorRing<D, const N: usize> {
    descriptors: [D; N],
    /// Current index for processing
    current: usize,
}

impl<const RX: usize, const TX: usize, const BUF: usize> DmaEngine<RX, TX, BUF> {
    /// Create new DMA engine (const, can be used in statics)
    pub const fn new() -> Self {
        Self {
            rx_ring: DescriptorRing::new(),
            tx_ring: DescriptorRing::new(),
            rx_buffers: [[0u8; BUF]; RX],
            tx_buffers: [[0u8; BUF]; TX],
            tx_flags: 0,
        }
    }
    
    /// Initialize descriptor chains
    /// Must be called before use
    /// Safety: Caller must ensure this is only called once and 
    /// hardware is properly initialized
    pub fn init(&mut self) {
        // Initialize RX descriptor chain
        for i in 0..RX {
            let next_idx = (i + 1) % RX;
            let buffer_ptr = self.rx_buffers[i].as_mut_ptr();
            let next_desc = &self.rx_ring.descriptors[next_idx] as *const _;
            
            self.rx_ring.descriptors[i].setup_chained(buffer_ptr, BUF, next_desc);
        }
        
        // Initialize TX descriptor chain
        for i in 0..TX {
            let next_idx = (i + 1) % TX;
            let buffer_ptr = self.tx_buffers[i].as_ptr();
            let next_desc = &self.tx_ring.descriptors[next_idx] as *const _;
            
            self.tx_ring.descriptors[i].setup_chained(buffer_ptr, next_desc);
        }
        
        self.rx_ring.current = 0;
        self.tx_ring.current = 0;
        
        // Set descriptor base addresses in hardware
        self.set_descriptor_addresses();
    }
    
    /// Reset descriptor chains to initial state
    pub fn reset(&mut self) {
        self.rx_ring.current = 0;
        self.tx_ring.current = 0;
        
        // Re-initialize all descriptors
        for i in 0..RX {
            self.rx_ring.descriptors[i].set_owned_by_dma();
        }
        
        for i in 0..TX {
            // Clear ownership - CPU owns all TX descriptors
            self.tx_ring.descriptors[i].tdes0.set(tdes0::SECOND_ADDR_CHAINED);
        }
        
        self.set_descriptor_addresses();
    }
    
    fn set_descriptor_addresses(&self) {
        let rx_base = &self.rx_ring.descriptors[0] as *const _ as u32;
        let tx_base = &self.tx_ring.descriptors[0] as *const _ as u32;
        
        // Write to DMA registers
        unsafe {
            core::ptr::write_volatile((DMA_BASE + 0x0C) as *mut u32, rx_base);
            core::ptr::write_volatile((DMA_BASE + 0x10) as *mut u32, tx_base);
        }
    }
}
```

### Transmit Operation

```rust
impl<const RX: usize, const TX: usize, const BUF: usize> DmaEngine<RX, TX, BUF> {
    /// Transmit a frame
    /// Returns number of bytes transmitted or error
    pub fn transmit(&mut self, data: &[u8]) -> Result<usize, Error> {
        if data.is_empty() {
            return Err(Error::InvalidLength);
        }
        
        if data.len() > BUF * TX {
            return Err(Error::FrameTooLarge);
        }
        
        // Calculate number of descriptors needed
        let desc_count = (data.len() + BUF - 1) / BUF;
        
        // Check if enough descriptors available
        if !self.tx_descriptors_available(desc_count) {
            return Err(Error::NoDescriptorsAvailable);
        }
        
        let mut remaining = data.len();
        let mut offset = 0usize;
        
        for i in 0..desc_count {
            let idx = (self.tx_ring.current + i) % TX;
            let desc = &self.tx_ring.descriptors[idx];
            
            // Check ownership
            if desc.is_owned_by_dma() {
                return Err(Error::DescriptorBusy);
            }
            
            // Calculate chunk size
            let chunk_size = core::cmp::min(remaining, BUF);
            
            // Copy data to buffer
            let buffer = &mut self.tx_buffers[idx][..chunk_size];
            buffer.copy_from_slice(&data[offset..offset + chunk_size]);
            
            // Configure descriptor
            let is_first = i == 0;
            let is_last = i == desc_count - 1;
            desc.prepare_tx(chunk_size, is_first, is_last);
            
            // Apply any global TX flags
            if is_first {
                let flags = desc.tdes0.get() | (self.tx_flags & TDES0_FS_CTRL_FLAGS_MASK);
                desc.tdes0.set(flags);
            }
            if is_last {
                let flags = desc.tdes0.get() | (self.tx_flags & TDES0_LS_CTRL_FLAGS_MASK);
                desc.tdes0.set(flags);
            }
            
            remaining -= chunk_size;
            offset += chunk_size;
        }
        
        // Give descriptors to DMA (in reverse order to avoid race)
        for i in (0..desc_count).rev() {
            let idx = (self.tx_ring.current + i) % TX;
            self.tx_ring.descriptors[idx].set_owned_by_dma();
        }
        
        // Update current index
        self.tx_ring.current = (self.tx_ring.current + desc_count) % TX;
        
        // Trigger DMA poll demand
        self.transmit_poll_demand();
        
        Ok(data.len())
    }
    
    /// Check if N descriptors are available for TX
    fn tx_descriptors_available(&self, count: usize) -> bool {
        for i in 0..count {
            let idx = (self.tx_ring.current + i) % TX;
            if self.tx_ring.descriptors[idx].is_owned_by_dma() {
                return false;
            }
        }
        true
    }
    
    /// Issue transmit poll demand to DMA
    fn transmit_poll_demand(&self) {
        unsafe {
            core::ptr::write_volatile((DMA_BASE + 0x04) as *mut u32, 0);
        }
    }
}
```

### Receive Operation

```rust
impl<const RX: usize, const TX: usize, const BUF: usize> DmaEngine<RX, TX, BUF> {
    /// Check if a complete frame is available
    pub fn frame_available(&self) -> bool {
        let desc = &self.rx_ring.descriptors[self.rx_ring.current];
        !desc.is_owned_by_dma() && desc.is_last_descriptor()
    }
    
    /// Get length of next available frame
    pub fn peek_frame_length(&self) -> Option<usize> {
        let desc = &self.rx_ring.descriptors[self.rx_ring.current];
        
        if desc.is_owned_by_dma() {
            return None;
        }
        
        if desc.has_error() {
            return None;
        }
        
        // Find first descriptor
        let mut idx = self.rx_ring.current;
        while !self.rx_ring.descriptors[idx].is_first_descriptor() {
            idx = if idx == 0 { RX - 1 } else { idx - 1 };
        }
        
        // Get length from last descriptor
        let mut len_idx = idx;
        while !self.rx_ring.descriptors[len_idx].is_last_descriptor() {
            len_idx = (len_idx + 1) % RX;
        }
        
        // Frame length includes CRC, subtract 4
        let frame_len = self.rx_ring.descriptors[len_idx].frame_length();
        Some(frame_len.saturating_sub(4))  // Remove CRC
    }
    
    /// Receive a frame into the provided buffer
    /// Returns actual frame length or error
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        // Find frame start
        let start_idx = self.find_frame_start()?;
        
        // Get frame length
        let frame_len = self.get_frame_length(start_idx)?;
        let copy_len = frame_len.saturating_sub(4);  // Exclude CRC
        
        if buffer.len() < copy_len {
            // Flush frame and return error
            self.flush_current_frame();
            return Err(Error::BufferTooSmall);
        }
        
        // Copy data from descriptors
        let mut copied = 0usize;
        let mut idx = start_idx;
        
        loop {
            let desc = &self.rx_ring.descriptors[idx];
            
            if desc.is_owned_by_dma() {
                // Incomplete frame
                return Err(Error::IncompleteFrame);
            }
            
            let chunk_start = copied;
            let chunk_len = core::cmp::min(BUF, copy_len - copied);
            
            if chunk_len > 0 {
                buffer[chunk_start..chunk_start + chunk_len]
                    .copy_from_slice(&self.rx_buffers[idx][..chunk_len]);
                copied += chunk_len;
            }
            
            let is_last = desc.is_last_descriptor();
            
            // Return descriptor to DMA
            desc.set_owned_by_dma();
            
            if is_last {
                break;
            }
            
            idx = (idx + 1) % RX;
        }
        
        // Update current index
        self.rx_ring.current = (idx + 1) % RX;
        
        // Issue receive poll demand
        self.receive_poll_demand();
        
        Ok(copied)
    }
    
    /// Flush current frame (discard erroneous frame)
    pub fn flush_current_frame(&mut self) {
        let mut idx = self.rx_ring.current;
        
        loop {
            let desc = &self.rx_ring.descriptors[idx];
            
            if desc.is_owned_by_dma() {
                break;
            }
            
            let is_last = desc.is_last_descriptor();
            desc.set_owned_by_dma();
            
            idx = (idx + 1) % RX;
            
            if is_last {
                break;
            }
        }
        
        self.rx_ring.current = idx;
        self.receive_poll_demand();
    }
    
    /// Count remaining complete frames in RX ring
    pub fn remaining_frames(&self) -> u32 {
        let mut count = 0u32;
        let mut idx = self.rx_ring.current;
        
        for _ in 0..RX {
            let desc = &self.rx_ring.descriptors[idx];
            
            if desc.is_owned_by_dma() {
                break;
            }
            
            if desc.is_last_descriptor() {
                count += 1;
            }
            
            idx = (idx + 1) % RX;
        }
        
        count
    }
    
    fn receive_poll_demand(&self) {
        unsafe {
            core::ptr::write_volatile((DMA_BASE + 0x08) as *mut u32, 0);
        }
    }
}
```

---

## MAC Driver

### Configuration

```rust
/// Speed configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Mbps10,
    Mbps100,
}

/// Duplex configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duplex {
    Half,
    Full,
}

/// PHY interface type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyInterface {
    Mii,
    Rmii,
}

/// Clock mode for RMII
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmiiClockMode {
    /// External clock input
    ExternalInput { gpio: u8 },
    /// Internal clock output
    InternalOutput { gpio: u8 },
}

/// DMA burst length configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DmaBurstLen {
    Burst1 = 1,
    Burst2 = 2,
    Burst4 = 4,
    Burst8 = 8,
    Burst16 = 16,
    Burst32 = 32,
}

/// SMI (MDIO) GPIO pins
#[derive(Debug, Clone, Copy)]
pub struct SmiPins {
    pub mdc: u8,
    pub mdio: u8,
}

/// RMII data GPIO pins
#[derive(Debug, Clone, Copy)]
pub struct RmiiPins {
    pub tx_en: u8,
    pub txd0: u8,
    pub txd1: u8,
    pub crs_dv: u8,
    pub rxd0: u8,
    pub rxd1: u8,
}

/// Complete EMAC configuration
#[derive(Debug, Clone)]
pub struct EmacConfig {
    pub phy_interface: PhyInterface,
    pub rmii_clock: Option<RmiiClockMode>,
    pub smi_pins: SmiPins,
    pub rmii_pins: Option<RmiiPins>,
    pub dma_burst_len: DmaBurstLen,
    pub sw_reset_timeout_ms: u32,
    pub mdc_freq_hz: u32,
}

impl Default for EmacConfig {
    fn default() -> Self {
        Self {
            phy_interface: PhyInterface::Rmii,
            rmii_clock: Some(RmiiClockMode::ExternalInput { gpio: 0 }),
            smi_pins: SmiPins { mdc: 23, mdio: 18 },
            rmii_pins: Some(RmiiPins {
                tx_en: 21,
                txd0: 19,
                txd1: 22,
                crs_dv: 27,
                rxd0: 25,
                rxd1: 26,
            }),
            dma_burst_len: DmaBurstLen::Burst32,
            sw_reset_timeout_ms: 100,
            mdc_freq_hz: 2_500_000,
        }
    }
}
```

### Driver State Machine

```rust
/// EMAC driver state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Not initialized
    Uninitialized,
    /// Initialized but not started
    Initialized,
    /// Running (TX/RX enabled)
    Running,
    /// Stopped
    Stopped,
}

/// Main EMAC driver
pub struct Emac<const RX: usize, const TX: usize, const BUF: usize> {
    dma: DmaEngine<RX, TX, BUF>,
    config: EmacConfig,
    state: State,
    mac_addr: [u8; 6],
    speed: Speed,
    duplex: Duplex,
    flow_control_enabled: bool,
}

impl<const RX: usize, const TX: usize, const BUF: usize> Emac<RX, TX, BUF> {
    /// Create new EMAC instance (const, for static allocation)
    pub const fn new() -> Self {
        Self {
            dma: DmaEngine::new(),
            config: EmacConfig::const_default(),
            state: State::Uninitialized,
            mac_addr: [0u8; 6],
            speed: Speed::Mbps100,
            duplex: Duplex::Full,
            flow_control_enabled: false,
        }
    }
    
    /// Initialize the EMAC with given configuration
    pub fn init(&mut self, config: EmacConfig) -> Result<(), Error> {
        if self.state != State::Uninitialized {
            return Err(Error::AlreadyInitialized);
        }
        
        self.config = config;
        
        // Enable bus clock
        self.enable_bus_clock();
        
        // Reset registers
        self.reset_registers();
        
        // Perform software reset
        self.software_reset()?;
        
        // Configure SMI clock
        self.configure_smi_clock()?;
        
        // Initialize MAC defaults
        self.init_mac_defaults();
        
        // Initialize DMA defaults
        self.init_dma_defaults();
        
        // Initialize GPIO
        self.configure_gpio()?;
        
        // Configure clocks
        self.configure_clocks()?;
        
        // Read MAC address from eFuse
        self.read_efuse_mac_addr();
        
        // Set MAC address in hardware
        self.set_mac_addr_hw(&self.mac_addr);
        
        // Initialize DMA engine
        self.dma.init();
        
        self.state = State::Initialized;
        Ok(())
    }
    
    /// Start EMAC (enable TX and RX)
    pub fn start(&mut self) -> Result<(), Error> {
        if self.state != State::Initialized && self.state != State::Stopped {
            return Err(Error::InvalidState);
        }
        
        // Reset DMA descriptors
        self.dma.reset();
        
        // Clear all pending interrupts
        self.clear_all_interrupts();
        
        // Enable interrupts
        self.enable_interrupts();
        
        // Enable TX in MAC
        self.mac_tx_enable(true);
        
        // Start DMA TX
        self.dma_tx_start(true);
        
        // Start DMA RX
        self.dma_rx_start(true);
        
        // Enable RX in MAC
        self.mac_rx_enable(true);
        
        self.state = State::Running;
        Ok(())
    }
    
    /// Stop EMAC
    pub fn stop(&mut self) -> Result<(), Error> {
        if self.state != State::Running {
            return Err(Error::InvalidState);
        }
        
        // Stop DMA TX
        self.dma_tx_start(false);
        
        // Wait for TX to complete (with timeout)
        self.wait_tx_complete()?;
        
        // Stop DMA RX
        self.dma_rx_start(false);
        
        // Disable MAC TX/RX
        self.mac_tx_enable(false);
        self.mac_rx_enable(false);
        
        // Flush TX FIFO
        self.flush_tx_fifo()?;
        
        // Disable interrupts
        self.disable_interrupts();
        
        self.state = State::Stopped;
        Ok(())
    }
    
    /// Transmit a frame
    pub fn transmit(&mut self, data: &[u8]) -> Result<usize, Error> {
        if self.state != State::Running {
            return Err(Error::InvalidState);
        }
        self.dma.transmit(data)
    }
    
    /// Receive a frame
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        if self.state != State::Running {
            return Err(Error::InvalidState);
        }
        self.dma.receive(buffer)
    }
    
    /// Check if frame is available
    pub fn frame_available(&self) -> bool {
        self.dma.frame_available()
    }
}
```

### MDIO Interface

```rust
impl<const RX: usize, const TX: usize, const BUF: usize> Emac<RX, TX, BUF> {
    /// Write to PHY register via MDIO
    pub fn write_phy_reg(&mut self, phy_addr: u8, reg: u8, value: u16) -> Result<(), Error> {
        // Wait for not busy
        self.wait_mii_not_busy()?;
        
        // Write data
        unsafe {
            core::ptr::write_volatile(
                (MAC_BASE + EMAC_MIIDATA_OFFSET) as *mut u32,
                value as u32
            );
        }
        
        // Write address and command
        let cmd = ((phy_addr as u32) << 11) |
                  ((reg as u32) << 6) |
                  EMAC_MII_WRITE |
                  EMAC_MII_BUSY |
                  self.get_mii_clock_divider();
                  
        unsafe {
            core::ptr::write_volatile(
                (MAC_BASE + EMAC_MIIADDR_OFFSET) as *mut u32,
                cmd
            );
        }
        
        // Wait for completion
        self.wait_mii_not_busy()
    }
    
    /// Read from PHY register via MDIO
    pub fn read_phy_reg(&mut self, phy_addr: u8, reg: u8) -> Result<u16, Error> {
        // Wait for not busy
        self.wait_mii_not_busy()?;
        
        // Write address and command (read)
        let cmd = ((phy_addr as u32) << 11) |
                  ((reg as u32) << 6) |
                  EMAC_MII_BUSY |
                  self.get_mii_clock_divider();
                  
        unsafe {
            core::ptr::write_volatile(
                (MAC_BASE + EMAC_MIIADDR_OFFSET) as *mut u32,
                cmd
            );
        }
        
        // Wait for completion
        self.wait_mii_not_busy()?;
        
        // Read data
        let value = unsafe {
            core::ptr::read_volatile((MAC_BASE + EMAC_MIIDATA_OFFSET) as *const u32)
        };
        
        Ok((value & 0xFFFF) as u16)
    }
    
    fn wait_mii_not_busy(&self) -> Result<(), Error> {
        for _ in 0..10000 {
            let addr = unsafe {
                core::ptr::read_volatile((MAC_BASE + EMAC_MIIADDR_OFFSET) as *const u32)
            };
            if (addr & EMAC_MII_BUSY) == 0 {
                return Ok(());
            }
            // Small delay (implementation-specific)
        }
        Err(Error::Timeout)
    }
}
```

---

## Error Handling

```rust
/// Error types (no heap allocation)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Operation timed out
    Timeout,
    /// Invalid state for operation
    InvalidState,
    /// Already initialized
    AlreadyInitialized,
    /// Buffer too small for frame
    BufferTooSmall,
    /// Frame too large for buffers
    FrameTooLarge,
    /// Invalid frame length
    InvalidLength,
    /// No descriptors available
    NoDescriptorsAvailable,
    /// Descriptor is busy (owned by DMA)
    DescriptorBusy,
    /// Incomplete frame received
    IncompleteFrame,
    /// Frame has errors
    FrameError,
    /// PHY communication error
    PhyError,
    /// Clock configuration error
    ClockError,
    /// GPIO configuration error
    GpioError,
    /// Hardware error
    HardwareError,
}
```

---

## Interrupt Handling

```rust
/// Interrupt status flags
#[derive(Debug, Clone, Copy, Default)]
pub struct InterruptStatus {
    pub tx_complete: bool,
    pub tx_stopped: bool,
    pub tx_buf_unavailable: bool,
    pub tx_underflow: bool,
    pub rx_complete: bool,
    pub rx_stopped: bool,
    pub rx_buf_unavailable: bool,
    pub rx_overflow: bool,
    pub fatal_bus_error: bool,
}

impl<const RX: usize, const TX: usize, const BUF: usize> Emac<RX, TX, BUF> {
    /// Get current interrupt status
    pub fn get_interrupt_status(&self) -> InterruptStatus {
        let status = unsafe {
            core::ptr::read_volatile((DMA_BASE + 0x14) as *const u32)
        };
        
        InterruptStatus {
            tx_complete: (status & DmaStatus::TX_INT) != 0,
            tx_stopped: (status & DmaStatus::TX_STOPPED) != 0,
            tx_buf_unavailable: (status & DmaStatus::TX_BUF_UNAVAIL) != 0,
            tx_underflow: (status & DmaStatus::TX_UNDERFLOW) != 0,
            rx_complete: (status & DmaStatus::RX_INT) != 0,
            rx_stopped: (status & DmaStatus::RX_STOPPED) != 0,
            rx_buf_unavailable: (status & DmaStatus::RX_BUF_UNAVAIL) != 0,
            rx_overflow: (status & DmaStatus::RX_OVERFLOW) != 0,
            fatal_bus_error: (status & DmaStatus::FATAL_BUS_ERR) != 0,
        }
    }
    
    /// Clear interrupt flags
    pub fn clear_interrupts(&self, status: InterruptStatus) {
        let mut flags = 0u32;
        
        if status.tx_complete { flags |= DmaStatus::TX_INT; }
        if status.tx_stopped { flags |= DmaStatus::TX_STOPPED; }
        if status.tx_buf_unavailable { flags |= DmaStatus::TX_BUF_UNAVAIL; }
        if status.tx_underflow { flags |= DmaStatus::TX_UNDERFLOW; }
        if status.rx_complete { flags |= DmaStatus::RX_INT; }
        if status.rx_stopped { flags |= DmaStatus::RX_STOPPED; }
        if status.rx_buf_unavailable { flags |= DmaStatus::RX_BUF_UNAVAIL; }
        if status.rx_overflow { flags |= DmaStatus::RX_OVERFLOW; }
        if status.fatal_bus_error { flags |= DmaStatus::FATAL_BUS_ERR; }
        
        // Write to clear (W1C)
        unsafe {
            core::ptr::write_volatile((DMA_BASE + 0x14) as *mut u32, flags);
        }
    }
    
    /// Handle interrupt (call from ISR)
    /// Returns true if there's work to do (frames to process)
    #[inline(always)]
    pub fn handle_interrupt(&self) -> bool {
        let status = self.get_interrupt_status();
        self.clear_interrupts(status);
        
        status.rx_complete || status.tx_complete
    }
}
```

---

## Integration Traits

### smoltcp Integration

```rust
#[cfg(feature = "smoltcp")]
mod smoltcp_impl {
    use super::*;
    use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
    use smoltcp::time::Instant;
    
    pub struct EmacRxToken<'a, const RX: usize, const TX: usize, const BUF: usize> {
        emac: &'a mut Emac<RX, TX, BUF>,
    }
    
    pub struct EmacTxToken<'a, const RX: usize, const TX: usize, const BUF: usize> {
        emac: &'a mut Emac<RX, TX, BUF>,
    }
    
    impl<'a, const RX: usize, const TX: usize, const BUF: usize> RxToken 
        for EmacRxToken<'a, RX, TX, BUF> 
    {
        fn consume<R, F>(self, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            // Use stack-allocated buffer (max MTU)
            let mut buffer = [0u8; 1522];
            let len = self.emac.receive(&mut buffer).unwrap_or(0);
            f(&mut buffer[..len])
        }
    }
    
    impl<'a, const RX: usize, const TX: usize, const BUF: usize> TxToken 
        for EmacTxToken<'a, RX, TX, BUF> 
    {
        fn consume<R, F>(self, len: usize, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            // Use stack-allocated buffer
            let mut buffer = [0u8; 1522];
            let result = f(&mut buffer[..len]);
            let _ = self.emac.transmit(&buffer[..len]);
            result
        }
    }
    
    impl<const RX: usize, const TX: usize, const BUF: usize> Device 
        for Emac<RX, TX, BUF> 
    {
        type RxToken<'a> = EmacRxToken<'a, RX, TX, BUF> where Self: 'a;
        type TxToken<'a> = EmacTxToken<'a, RX, TX, BUF> where Self: 'a;
        
        fn receive(&mut self, _timestamp: Instant) 
            -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> 
        {
            if self.frame_available() {
                Some((
                    EmacRxToken { emac: self },
                    EmacTxToken { emac: self },
                ))
            } else {
                None
            }
        }
        
        fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
            Some(EmacTxToken { emac: self })
        }
        
        fn capabilities(&self) -> DeviceCapabilities {
            let mut caps = DeviceCapabilities::default();
            caps.medium = Medium::Ethernet;
            caps.max_transmission_unit = 1500;
            caps.max_burst_size = Some(1);
            caps
        }
    }
}
```

---

## Feature Flags

```toml
[features]
default = ["esp32"]

# Target selection (mutually exclusive)
esp32 = []
esp32p4 = []

# Optional integrations
smoltcp = ["dep:smoltcp"]           # smoltcp network stack integration
critical-section = ["dep:critical-section"]  # ISR-safe SharedEmac wrapper

# Advanced features
ptp = []           # IEEE 1588 PTP timestamping (ESP32-P4 only)
flow-control = []  # Hardware flow control
checksum = []      # Hardware checksum offload

# Development
defmt = ["dep:defmt"]  # defmt logging support

[dependencies]
# Required: embedded-hal traits for ecosystem compatibility
embedded-hal = { version = "1.0" }
```

### `critical-section` Feature

The `critical-section` feature enables the `SharedEmac` wrapper, which provides
interrupt-safe access to the EMAC driver:

```rust
use esp32_emac::{SharedEmac, EmacConfig};

// Safe static allocation - no `unsafe` needed!
static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();

fn main() {
    // Initialize within critical section
    EMAC.with(|emac| {
        emac.init(EmacConfig::default()).unwrap();
        emac.start().unwrap();
    });
}

#[interrupt]
fn EMAC_IRQ() {
    // Safe access from ISR - interrupts disabled during access
    EMAC.with(|emac| {
        let status = emac.read_interrupt_status();
        emac.clear_interrupts(status);
    });
}
```

**Implementation Note**: The critical section implementation is provided by your
HAL crate (e.g., `esp-hal`). Enable the appropriate feature:

```toml
[dependencies]
esp-hal = { version = "...", features = ["critical-section"] }
esp32-emac = { version = "...", features = ["critical-section"] }
```

---

## Usage Example

```rust
#![no_std]
#![no_main]

use esp32_emac::{Emac, EmacConfig, PhyInterface, RmiiClockMode};

// Static allocation - placed in DMA-capable memory
#[link_section = ".dram1"]
static mut EMAC: Emac<10, 10, 1600> = Emac::new();

#[entry]
fn main() -> ! {
    // Get mutable reference (unsafe, single-threaded assumption)
    let emac = unsafe { &mut EMAC };
    
    // Configure
    let config = EmacConfig {
        phy_interface: PhyInterface::Rmii,
        rmii_clock: Some(RmiiClockMode::ExternalInput { gpio: 0 }),
        ..EmacConfig::default()
    };
    
    // Initialize
    emac.init(config).expect("EMAC init failed");
    
    // Initialize PHY (implementation-specific)
    init_phy(emac);
    
    // Start
    emac.start().expect("EMAC start failed");
    
    // Main loop
    let mut rx_buffer = [0u8; 1522];
    
    loop {
        // Check for received frames
        if emac.frame_available() {
            match emac.receive(&mut rx_buffer) {
                Ok(len) => {
                    // Process frame
                    process_frame(&rx_buffer[..len]);
                }
                Err(e) => {
                    // Handle error
                }
            }
        }
        
        // Handle interrupts if using interrupt-driven mode
        if emac.handle_interrupt() {
            // Wake up processing task
        }
    }
}

fn init_phy(emac: &mut Emac<10, 10, 1600>) {
    // Reset PHY
    emac.write_phy_reg(0, 0, 0x8000).unwrap();
    
    // Wait for reset complete
    while (emac.read_phy_reg(0, 0).unwrap() & 0x8000) != 0 {}
    
    // Enable auto-negotiation
    emac.write_phy_reg(0, 0, 0x1000).unwrap();
    
    // Wait for link
    while (emac.read_phy_reg(0, 1).unwrap() & 0x0004) == 0 {}
}
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tx_descriptor_layout() {
        assert_eq!(core::mem::size_of::<TxDescriptor>(), 32);
        assert_eq!(core::mem::align_of::<TxDescriptor>(), 4);
    }
    
    #[test]
    fn test_rx_descriptor_layout() {
        assert_eq!(core::mem::size_of::<RxDescriptor>(), 32);
        assert_eq!(core::mem::align_of::<RxDescriptor>(), 4);
    }
    
    #[test]
    fn test_emac_size() {
        // Verify total size is as expected
        let size = core::mem::size_of::<Emac<10, 10, 1600>>();
        assert!(size < 40_000); // Should be around 32KB
    }
}
```

### Integration Tests

- MDIO read/write to real PHY
- Loopback TX/RX test
- Performance benchmarks
- Stress tests (continuous TX/RX)

---

## Implementation Roadmap

### Phase 1: Foundation ✅ COMPLETE
- [x] Register definitions (DMA, MAC, EXT)
- [x] Descriptor structures
- [x] Basic memory layout
- [x] Error and config types

### Phase 2: DMA Engine ✅ COMPLETE
- [x] Descriptor ring management
- [x] TX implementation
- [x] RX implementation
- [x] Buffer management

### Phase 3: MAC Driver ✅ COMPLETE
- [x] Initialization sequence
- [x] Start/stop logic
- [x] MDIO interface
- [x] Interrupt handling

### Phase 4: HAL Integration ✅ COMPLETE
- [x] Clock configuration (`hal/clock.rs`)
- [x] GPIO setup (`hal/gpio.rs`)
- [x] Reset handling (`hal/reset.rs`)
- [x] MDIO/PHY helpers (`hal/mdio.rs`)
- [x] Delay provider trait

### Phase 5: Testing & Polish (In Progress)
- [ ] Unit tests
- [ ] Integration tests on hardware
- [x] smoltcp integration (`smoltcp.rs`)
- [x] Documentation (doc comments)

### Phase 6: smoltcp Integration ✅ COMPLETE
- [x] Add smoltcp as optional dependency
- [x] Implement `Device` trait
- [x] Implement `RxToken` and `TxToken`
- [x] Checksum capabilities

### Phase 7: Flow Control ✅ COMPLETE
- [x] FlowControlConfig with water marks
- [x] PAUSE frame transmission
- [x] Peer PAUSE ability detection
- [x] Software flow control logic

### Phase 8: Advanced Features (Future)
- [ ] IEEE 1588 PTP timestamps (ESP32-P4 only)
- [ ] Power management
- [ ] Sleep retention

---

## Feature Gap Analysis

This section documents features present in ESP-IDF's EMAC implementation that are not yet
implemented in this Rust driver. Use this as a roadmap for future development.

### ✅ Implemented Features (Parity with ESP-IDF)

| Feature | Module | Notes |
|---------|--------|-------|
| Basic TX/RX | `dma.rs`, `mac.rs` | Full implementation |
| DMA descriptor rings | `dma.rs` | Enhanced descriptor format |
| MDIO PHY read/write | `mac.rs` | `read_phy_reg()`, `write_phy_reg()` |
| MAC address set/get | `mac.rs` | Primary address only |
| Speed configuration | `mac.rs` | 10/100 Mbps |
| Duplex configuration | `mac.rs` | Half/Full |
| Promiscuous mode | `mac.rs` | `set_promiscuous()` |
| Pass all multicast | `mac.rs` | `set_pass_all_multicast()` |
| Interrupt handling | `mac.rs` | `InterruptStatus`, `handle_interrupt()` |
| MII/RMII interface | `config.rs`, `mac.rs` | GPIO routing in HAL |
| RMII clock modes | `config.rs` | Internal/external |
| RX checksum offload | `config.rs` | Hardware IP/TCP/UDP |
| TX checksum insertion | `config.rs`, `descriptor.rs` | 4 modes |
| DMA burst length | `config.rs` | 1-32 beats |
| Flow control (PAUSE) | `mac.rs`, `config.rs` | Software-driven with water marks |
| MAC address filtering | `mac.rs`, `register/mac.rs` | Up to 4 additional filter slots |
| Hash table filtering | `mac.rs`, `register/mac.rs` | 64-bit CRC-32 hash for multicast |
| VLAN tag filtering | `mac.rs`, `register/mac.rs` | 802.1Q C-VLAN and S-VLAN support |
| smoltcp integration | `smoltcp.rs` | `Device` trait |

### ❌ Missing Features (ESP32 Classic)

#### Priority: LOW

| Feature | ESP-IDF Function | Effort | Description |
|---------|------------------|--------|-------------|
| **Scatter-Gather TX** | `emac_esp32_transmit_ctrl_vargs()` | High | TX from multiple non-contiguous buffers |
| **TX Descriptor Flags** | `ETH_MAC_ESP_CMD_SET_TDES0_CFG_BITS` | Low | Set custom TDES0 control bits |
| **Register Debug Dump** | `emac_esp_dump_hal_registers()` | Low | Dump all registers for debugging |
| **Link Status Callback** | `emac_esp32_set_link()` | Low | Auto start/stop on link change |

### ⚠️ ESP32-P4 Only Features (Not Applicable to ESP32 Classic)

These features require `SOC_EMAC_IEEE1588V2_SUPPORTED` which is only available on ESP32-P4:

| Feature | Description |
|---------|-------------|
| IEEE 1588v2 PTP | Precision Time Protocol hardware timestamps |
| `esp_eth_mac_ptp_enable()` | Enable PTP module |
| `esp_eth_mac_set_ptp_time()` | Set system time |
| `esp_eth_mac_get_ptp_time()` | Get system time |
| `esp_eth_mac_adj_ptp_freq()` | Adjust time base frequency |
| `esp_eth_mac_set_pps_out_gpio()` | Pulse-per-second output |
| TX/RX Timestamps | Per-frame nanosecond timestamps |

### 🔮 Future Considerations

| Feature | Notes |
|---------|-------|
| **Sleep Retention** | Requires ESP-IDF's `sleep_retention` module for PM |
| **APLL Clock Output** | GPIO0 clock output requires `esp_clock_output` driver |
| **Zero-Copy TX** | Would require unsafe API changes |
| **Async/Embassy Support** | Async receive/transmit with wakers |

---

## Implementation Priority Order

For achieving full ESP-IDF feature parity on ESP32 (classic):

1. ✅ ~~Flow Control (PAUSE frames)~~ - **DONE**
2. ✅ ~~MAC Address Filtering~~ - **DONE** (`add_mac_filter()`, `remove_mac_filter()`)
3. ✅ ~~Hash Table Filtering~~ - **DONE** (`add_hash_filter()`, CRC-32 hash)
4. ✅ ~~VLAN Tag Filtering~~ - **DONE** (`set_vlan_filter()`, C-VLAN/S-VLAN)
5. 🔲 Unit Tests - Descriptor layout, state machine tests
6. 🔲 Hardware Integration Tests - Real PHY, loopback

---

## Code Quality: Idiomatic Rust

This section documents Rust idioms and best practices applied to this codebase.

### ✅ Implemented Idioms

| Idiom | Files | Description |
|-------|-------|-------------|
| `#[must_use]` attributes | `error.rs`, `dma.rs`, `config.rs`, `descriptor/tx.rs`, `descriptor/rx.rs` | Pure functions that return values have `#[must_use]` to catch unused results |
| `Display` for errors | `error.rs` | `Error` enum implements `core::fmt::Display` via `as_str()` for idiomatic error printing |
| `Default` trait | `descriptor/tx.rs`, `descriptor/rx.rs`, `config.rs` | Types with sensible defaults implement `Default` |
| `#![deny(missing_docs)]` | `lib.rs` | All public items require documentation |
| `#![deny(unsafe_op_in_unsafe_fn)]` | `lib.rs` | Unsafe operations must be explicitly marked even inside unsafe functions |
| Const constructors | All modules | `const fn new()` enables static initialization without runtime overhead |
| Enum-based errors | `error.rs` | `Error` is a `Copy` enum with no heap allocation |
| `defmt::Format` derives | All public types | Conditional support for embedded debugging |

### 🔲 Remaining Idiom Gaps (Medium/Low Priority)

| Idiom | Current State | Recommendation |
|-------|---------------|----------------|
| **`VolatileCell` Sync safety** | Safety comment mentions atomicity | Clarify alignment requirements in safety documentation |
| **`From`/`Into` traits** | `InterruptStatus::from_raw()`/`to_raw()` | Could implement `From<u32>` and `From<InterruptStatus>` for `u32` |
| **Pointer provenance** | `as u32` pointer casts | Use `.addr()` method for Rust 2024 pointer provenance |
| **Const generic bounds** | No compile-time validation | Add static assertions for `RX_BUFS > 0`, `TX_BUFS > 0` |
| **Builder pattern** | `EmacConfig` uses struct literals | Builder pattern would improve configuration ergonomics |
| **`bitflags` crate** | Manual `const` bit definitions | `bitflags` provides type-safe bitfield operations |
| **`NonZero*` types** | Regular integer types | Use `NonZeroU32` for timeout values to enable niche optimization |

### Code Quality Principles

1. **No `std`**: `#![no_std]` - suitable for bare-metal embedded targets
2. **No allocator**: All memory is statically allocated via const generics
3. **Minimal unsafe**: Only used for hardware register access and DMA
4. **Feature flags**: Clean separation of optional functionality (`smoltcp`, `defmt`, `esp32p4`)

---

## esp-hal Integration Analysis

This section analyzes how integrating with [esp-hal](https://github.com/esp-rs/esp-hal) could reduce
the code surface of this EMAC driver while improving ecosystem compatibility.

### Current HAL Implementation

The EMAC driver currently contains a custom HAL layer in `src/hal/`:

| Module | Lines | Purpose |
|--------|-------|---------|
| `clock.rs` | ~230 | EMAC clock configuration (RMII/MII mode, internal/external clock) |
| `gpio.rs` | ~411 | GPIO pin traits and RMII/SMI pin configuration types |
| `reset.rs` | ~222 | Soft reset controller and state machine |
| `mdio.rs` | ~421 | MDIO/SMI bus for PHY register access |
| **Total** | **~1284** | Custom HAL code |

### esp-hal Provides

Based on [esp-hal 1.0.0 documentation](https://docs.rs/esp-hal/latest/esp_hal/), the following modules overlap with our HAL:

| esp-hal Module | Status | Can Replace |
|----------------|--------|-------------|
| `gpio` | ✅ Stable | GPIO configuration and pin types |
| `time` | ✅ Stable | `Duration`, `Instant` for delays |
| `delay` | ⚠️ Unstable | `DelayNs` trait implementation |
| `dma` | ⚠️ Unstable | Generic DMA descriptors/buffers (partial) |
| `clock` | ✅ Stable | CPU clock, but NOT peripheral clocks |
| `system` | ✅ Stable | Peripheral reset (partial) |
| `peripherals` | ✅ Stable | Peripheral singletons pattern |

### What esp-hal CANNOT Replace

The EMAC peripheral has unique requirements that esp-hal doesn't cover:

1. **EMAC-Specific Registers**: All of `register/mac.rs`, `register/dma.rs`, `register/ext.rs` - these are EMAC hardware registers not abstracted by esp-hal

2. **EMAC DMA Descriptors**: The DWMAC uses a specific descriptor format (TDES0-3, RDES0-3) that is different from esp-hal's generic DMA

3. **RMII/MII Clock Configuration**: The `EX_PHYINF_CONF` register controls EMAC-specific clocking not handled by esp-hal's clock module

4. **MDIO/SMI Protocol**: PHY communication over MDIO is EMAC-specific (GMACMIIADDR/GMACMIIDATA registers)

5. **MAC Core Logic**: All frame filtering, flow control, VLAN logic

### Integration Strategy

#### Option A: Minimal Integration (Recommended)

Use esp-hal for:
- **GPIO types**: Accept `esp_hal::gpio::*` types instead of custom pin traits
- **Delay trait**: Use `embedded_hal::delay::DelayNs` instead of custom `DelayProvider`
- **Peripheral singleton**: Accept `peripherals.EMAC` from esp-hal init

Keep custom:
- All EMAC register definitions
- DMA descriptor implementation
- MDIO implementation (but accept esp-hal GPIO for pins)
- Clock configuration (EMAC-specific)

**Code reduction**: ~300-400 lines (GPIO trait boilerplate)
**Breaking change**: Requires esp-hal as dependency

#### Option B: Full esp-hal Integration

Redesign the driver as an esp-hal peripheral driver:
- Follow esp-hal driver patterns (blocking/async modes)
- Use esp-hal's peripheral singleton system
- Integrate with esp-hal's interrupt handling
- Potentially contribute upstream to esp-hal

**Code reduction**: ~500-600 lines
**Breaking change**: Major API redesign, ties to esp-hal version

#### Option C: Keep Standalone (Current)

Maintain the standalone HAL for maximum flexibility:
- Works without esp-hal dependency
- Can be used with any GPIO/delay implementation via traits
- Self-contained, no version coupling

**Code reduction**: 0 lines
**Advantage**: Maximum portability

### Recommended Approach

**Option A (Minimal Integration)** is recommended because:

1. **GPIO simplification**: The current `gpio.rs` (~411 lines) is mostly trait definitions and pin configuration types that esp-hal already provides

2. **Delay simplification**: Replace `DelayProvider` trait with `embedded_hal::delay::DelayNs`

3. **Ecosystem compatibility**: Users already using esp-hal can pass their GPIO and delay types directly

4. **Minimal coupling**: Only depends on stable esp-hal APIs (`gpio`, `time`)

### Implementation Plan for esp-hal Integration

```
Phase 1: embedded-hal Integration ✅ COMPLETE
├── Make embedded-hal a required dependency (not optional)
├── Use embedded_hal::delay::DelayNs directly (no custom DelayProvider)
├── Use embedded_hal::digital::OutputPin for PHY reset pin
├── Remove SpinDelay, EmbeddedHalDelay wrapper types
├── Remove RmiiPins/SmiPins from public API (fixed by hardware)
└── ~150 lines removed, cleaner API

Phase 2: GPIO Documentation (Current State) ✅ COMPLETE
├── gpio.rs reduced to documentation + constants only (~70 lines)
├── esp32_gpio module documents fixed RMII pin assignments
├── No configurable GPIO in public API (pins are hardware-fixed)
└── PHY reset pin uses embedded_hal::digital::OutputPin

Phase 3: Peripheral Singleton (Future - Optional)
├── Accept peripherals.EMAC from esp-hal init
├── Enforce single-instance pattern via type system
├── Integrate with esp-hal's interrupt handling
└── ~50 lines changed in mac.rs

Phase 4: Async Support (Future - Optional)
├── Add async receive/transmit methods
├── Integrate with embassy executor
├── Use esp-hal's async primitives
└── Major feature addition
```

### Dependency Considerations

| Dependency | Adds | Benefits |
|------------|------|----------|
| `esp-hal` | ~large | Full ecosystem, maintained by Espressif |
| `embedded-hal` | ~small | Trait compatibility, no runtime cost |

**Current Status**: `embedded-hal` is a required dependency. `esp-hal` integration is through trait compatibility, not direct dependency.

### Feature Flag Design

```toml
[features]
default = ["esp32"]
esp32 = []
esp32p4 = []
defmt = ["dep:defmt"]
smoltcp = ["dep:smoltcp"]

# esp-hal integration (optional)
esp-hal = ["dep:esp-hal", "embedded-hal"]

# Standalone mode (current behavior)
standalone = []  # default when esp-hal not enabled
```

### Summary

| Approach | Lines Removed | Breaking Change | Ecosystem Fit |
|----------|---------------|-----------------|---------------|
| Standalone (current) | 0 | No | ⭐⭐ |
| Minimal esp-hal | ~300-400 | Minor | ⭐⭐⭐⭐ |
| Full esp-hal | ~500-600 | Major | ⭐⭐⭐⭐⭐ |

The EMAC-specific code (registers, DMA descriptors, MAC logic, MDIO) must remain regardless of esp-hal integration. The savings come from removing generic GPIO and delay abstractions that esp-hal already provides.

### ✅ Phase 1 Implementation Status: COMPLETE

Phase 1 (embedded-hal Integration) has been fully implemented with a cleaner approach than originally planned:

#### Design Decision: embedded-hal Required (Not Optional)

Instead of making `embedded-hal` optional with a fallback `SpinDelay`, we made it a **required dependency**:

```toml
[dependencies]
# embedded-hal traits for ecosystem compatibility (REQUIRED)
embedded-hal = { version = "1.0" }
```

**Rationale:**
- Users should provide their own delay from their HAL (esp-hal, etc.)
- Removes need for CPU frequency guessing in `SpinDelay`
- Cleaner API with no wrapper types needed
- `embedded-hal` is zero-cost (traits only, no runtime overhead)

#### What Was Removed

| Item | Lines | Reason |
|------|-------|--------|
| `DelayProvider` trait | ~15 | Replaced by `embedded_hal::delay::DelayNs` |
| `SpinDelay` struct | ~30 | Users provide their own delay |
| `EmbeddedHalDelay` wrapper | ~70 | No longer needed - use DelayNs directly |
| `SmiPins` struct | ~20 | Fixed by hardware, removed from public API |
| `RmiiPins` struct | ~35 | Fixed by hardware, removed from public API |
| `with_smi_pins()` builder | ~5 | Removed with SmiPins |
| `with_rmii_pins()` builder | ~5 | Removed with RmiiPins |
| `ESP32_DEFAULT_CPU_MHZ` const | ~1 | Only used by SpinDelay |
| `SPIN_LOOP_CYCLES` const | ~1 | Only used by SpinDelay |
| **Total** | **~180** | Cleaner API |

#### What Was Added

| Item | Lines | Purpose |
|------|-------|---------|
| `Lan8720aWithReset<RST>` | ~150 | PHY driver with `OutputPin` reset pin |
| `BorrowedDelay` helper | ~10 | Internal wrapper for `&mut D` as `DelayNs` |
| Pin documentation comments | ~20 | Documents fixed hardware pins in config.rs |
| **Total** | **~180** | esp-hal compatible features |

#### Current API

**EMAC Initialization:**
```rust
use esp_hal::delay::Delay;
use esp32_emac::{Emac, EmacConfig};

static mut EMAC: Emac<10, 10, 1600> = Emac::new();
let emac = unsafe { &mut EMAC };

let mut delay = Delay::new();
emac.init(EmacConfig::default(), &mut delay)?;
```

**MDIO Controller:**
```rust
use esp_hal::delay::Delay;
use esp32_emac::MdioController;

let mut delay = Delay::new();
let mut mdio = MdioController::new(&mut delay);
```

**PHY with Hardware Reset (Optional):**
```rust
use esp_hal::delay::Delay;
use esp_hal::gpio::Output;
use esp32_emac::phy::{Lan8720aWithReset, PhyDriver};

let reset_pin: Output<'_> = /* gpio5.into_push_pull_output() */;
let mut phy = Lan8720aWithReset::new(0, reset_pin);
phy.hardware_reset(&mut delay)?;
phy.init(&mut mdio)?;
```

**PHY without Reset Pin:**
```rust
use esp32_emac::phy::{Lan8720a, PhyDriver};

let mut phy = Lan8720a::new(0);
phy.init(&mut mdio)?;
```

#### File Changes Summary

| File | Change |
|------|--------|
| `Cargo.toml` | `embedded-hal` now required (not optional) |
| `hal/clock.rs` | Removed `DelayProvider`, `SpinDelay`, `EmbeddedHalDelay` (~115 lines) |
| `hal/mdio.rs` | `MdioController<D: DelayNs>` (was `DelayProvider`) |
| `hal/reset.rs` | `ResetController<D: DelayNs>`, `full_reset<D: DelayNs>()` |
| `mac.rs` | `init()` now takes `delay: D` parameter |
| `config.rs` | Removed `SmiPins`, `RmiiPins` from public API |
| `lib.rs` | Updated exports, removed pin types |
| `phy/lan8720a.rs` | Added `Lan8720aWithReset<RST: OutputPin>` |
| `phy/mod.rs` | Export `Lan8720aWithReset` |
| `hal/gpio.rs` | Reduced to documentation + constants (~70 lines) |
| `constants.rs` | Removed `ESP32_DEFAULT_CPU_MHZ`, `SPIN_LOOP_CYCLES` |

#### GPIO Status

The ESP32 EMAC RMII interface uses **dedicated internal routing** - GPIO pins are fixed:

| Signal | GPIO | Notes |
|--------|------|-------|
| TXD0 | 19 | Fixed, internal routing |
| TXD1 | 22 | Fixed, internal routing |
| TX_EN | 21 | Fixed, internal routing |
| RXD0 | 25 | Fixed, internal routing |
| RXD1 | 26 | Fixed, internal routing |
| CRS_DV | 27 | Fixed, internal routing |
| REF_CLK | 0/16/17 | Configurable via `RmiiClockMode` |
| MDC | 23 | Default, handled by hardware |
| MDIO | 18 | Default, handled by hardware |

The only user-configurable GPIO is the **PHY reset pin** (optional), which uses `embedded_hal::digital::OutputPin`.

---

## Code Surface Reduction Analysis

Analysis of the codebase to identify unused, redundant, or over-engineered code that can be removed or simplified.

### File Size Summary (Updated)

| File | Lines | Purpose | Status |
|------|-------|---------|--------|
| `mac.rs` | ~1170 | Main driver | ✅ Essential |
| `register/mac.rs` | ~975 | MAC registers | ⚠️ Some unused constants |
| `dma.rs` | ~629 | DMA engine | ✅ Essential |
| `phy/lan8720a.rs` | ~650 | PHY driver | ✅ Essential |
| `register/dma.rs` | ~469 | DMA registers | ⚠️ Some unused constants |
| `descriptor/rx.rs` | ~374 | RX descriptor | ✅ Essential |
| `hal/mdio.rs` | ~366 | MDIO bus | ✅ Essential |
| `config.rs` | ~455 | Configuration | ✅ Essential |
| `descriptor/tx.rs` | ~311 | TX descriptor | ✅ Essential |
| `hal/clock.rs` | ~175 | Clock config | ✅ Essential |
| `hal/reset.rs` | ~210 | Reset controller | ✅ Essential |
| `hal/gpio.rs` | ~70 | Documentation only | ✅ Minimal |
| `sync.rs` | ~210 | SharedEmac (critical-section) | ✅ Optional |

### Category 1: Completed Cleanup

#### `hal/gpio.rs` - ✅ DONE

**Before:** ~368 lines of unused GPIO trait infrastructure
**After:** ~70 lines of documentation + `esp32_gpio` constants module

#### `hal/clock.rs` - ✅ DONE  

**Before:** ~288 lines with `DelayProvider`, `SpinDelay`, `EmbeddedHalDelay`
**After:** ~175 lines with just `ClockController` and `ClockState`

#### `config.rs` - ✅ DONE

**Before:** `SmiPins` and `RmiiPins` structs exposed in public API
**After:** Pin assignments documented as comments, removed from public API

### Category 2: Remaining Optimization Opportunities

#### Register Constants (LOW Priority)

Many register constants are defined for completeness but not used:

| File | Defined | Used | Unused |
|------|---------|------|--------|
| `register/dma.rs` | ~50 | ~35 | ~15 |
| `register/mac.rs` | ~80 | ~45 | ~35 |
| `GpioProvider` trait | 4 | Definition + impl + 2 exports only |
| `ExternalGpioProvider` | 2 | Definition + impl only |
| `RmiiGpioPins` | 11 | All internal to gpio.rs |
| `SmiGpioPins` | 10 | All internal to gpio.rs |
| `RmiiPinConfig` | 19 | All internal to gpio.rs |
| `SmiPinConfig` | 7 | All internal to gpio.rs |
| `GpioConfig` | 6 | 2 exports only |

**Root Cause:** ESP32 EMAC uses **dedicated internal routing** for RMII pins. The data interface (TXD0/TXD1/RXD0/RXD1/CRS_DV/TX_EN) is not configurable via GPIO matrix. SMI uses hardware GMACMIIADDR/GMACMIIDATA registers, not bit-banged GPIO.

**Recommendation:** Remove gpio.rs entirely or reduce to documentation stub (~50 lines).

**Savings:** ~300 lines

#### `hal/reset.rs::ResetController` - Unused by driver

**Finding:** `ResetController` is defined and exported but the main driver (`mac.rs`) performs its own inline reset via `software_reset()` method.

| Item | Driver Usage |
|------|--------------|
**Recommendation:** Keep - `ResetController` is now used by `mac.rs::software_reset()` and provides a clean API for users who need standalone reset capability.

### Category 2: Helper Functions (KEEP)

#### PHY Register Constants

The mdio.rs file defines many PHY register constants (BMCR, BMSR, etc.) that are standard IEEE 802.3 values. These are used by the PHY driver and useful for users developing custom PHY drivers.

**Recommendation:** Keep - essential for PHY driver development.

### Summary of Completed Reductions

| Item | Lines Removed | Status |
|------|---------------|--------|
| `gpio.rs` infrastructure | ~300 | ✅ DONE - reduced to ~70 lines |
| `DelayProvider` trait | ~15 | ✅ DONE - use `DelayNs` directly |
| `SpinDelay` struct | ~30 | ✅ DONE - users provide delay |
| `EmbeddedHalDelay` wrapper | ~70 | ✅ DONE - not needed |
| `SmiPins`/`RmiiPins` | ~55 | ✅ DONE - removed from public API |
| Spin loop constants | ~2 | ✅ DONE - not needed |
| **Total removed** | **~472** | |

### Remaining Optimization Opportunities

| Item | Lines | Priority | Status |
|------|-------|----------|--------|
| Unused register constants | ~50 | LOW | Keep (zero-cost, useful for debug) |
| `read_phy_status` helper | ~20 | LOW | Keep (useful for PHY development) |

**Current codebase:** Well-optimized, no significant dead code remaining.
   - Users may need them for advanced configuration

### Implementation Note

To implement reductions, remove `#![allow(dead_code)]` from lib.rs (already done) and let the compiler guide which items can be made private or removed.

---

## Hexagonal Architecture & esp-hal Integration

This section describes the architectural approach for integrating this EMAC driver into the Rust ESP ecosystem, particularly `esp-hal`.

### Hexagonal Architecture Overview

The hexagonal (ports and adapters) architecture separates the core domain logic from external dependencies through well-defined interfaces (ports). This enables:

- **Testability**: Mock adapters for unit testing
- **Portability**: Same core logic with different platform adapters
- **Flexibility**: Swap implementations without changing domain code

```
                                 ┌─────────────────────────────────────┐
                                 │         APPLICATION LAYER           │
                                 │   (smoltcp, embassy-net, user app)  │
                                 └──────────────────┬──────────────────┘
                                                    │
                            ┌───────────────────────┼───────────────────────┐
                            │                       │                       │
                            ▼                       ▼                       ▼
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│                                     DRIVING PORTS                                         │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────────────────┐   │
│  │     EthDevice       │  │     PhyDriver       │  │        LinkMonitor              │   │
│  │  (TX/RX frames)     │  │  (PHY management)   │  │   (link change callbacks)       │   │
│  └─────────────────────┘  └─────────────────────┘  └─────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                          ▼
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│                                   DOMAIN (Core)                                           │
│                                                                                          │
│   ┌────────────────────────────────────────────────────────────────────────────────┐     │
│   │                               Emac<RX, TX, BUF>                                │     │
│   │  ┌─────────────┐  ┌─────────────────┐  ┌──────────────┐  ┌───────────────┐    │     │
│   │  │ DmaEngine   │  │ DescriptorRing  │  │ MacFiltering │  │ FlowControl   │    │     │
│   │  └─────────────┘  └─────────────────┘  └──────────────┘  └───────────────┘    │     │
│   └────────────────────────────────────────────────────────────────────────────────┘     │
│                                                                                          │
│   ┌────────────────────────────────────────────────────────────────────────────────┐     │
│   │                            PHY Domain (IEEE 802.3)                             │     │
│   │  ┌─────────────┐  ┌─────────────────┐  ┌──────────────┐  ┌───────────────┐    │     │
│   │  │ LinkStatus  │  │ PhyCapabilities │  │  AN Logic    │  │ Error Types   │    │     │
│   │  └─────────────┘  └─────────────────┘  └──────────────┘  └───────────────┘    │     │
│   └────────────────────────────────────────────────────────────────────────────────┘     │
│                                                                                          │
└──────────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                          ▼
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│                                    DRIVEN PORTS                                           │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────────────────┐   │
│  │     MdioBus         │  │    DelayProvider    │  │      RegisterAccess             │   │
│  │ (PHY communication) │  │  (timing/delays)    │  │   (hardware registers)          │   │
│  └─────────────────────┘  └─────────────────────┘  └─────────────────────────────────┘   │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────────────────┐   │
│  │   ClockProvider     │  │    GpioProvider     │  │      InterruptController        │   │
│  │ (clock tree config) │  │  (pin mux/config)   │  │   (IRQ enable/disable)          │   │
│  └─────────────────────┘  └─────────────────────┘  └─────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                          ▼
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│                                     ADAPTERS                                              │
│                                                                                          │
│   Standalone (current)           │   esp-hal Integration        │   Testing              │
│  ┌─────────────────────────┐     │  ┌─────────────────────┐     │  ┌─────────────────┐  │
│  │ SpinDelay               │     │  │ embassy_time::Delay │     │  │ MockDelay       │  │
│  │ MdioController          │     │  │ esp_hal::gpio::*    │     │  │ MockMdio        │  │
│  │ Direct register access  │     │  │ Peripheral<EMAC>    │     │  │ FakeRegisters   │  │
│  └─────────────────────────┘     │  └─────────────────────┘     │  └─────────────────┘  │
│                                  │                              │                       │
└──────────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                          ▼
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│                                     HARDWARE                                              │
│  ┌───────────────────┐  ┌───────────────────┐  ┌───────────────────────────────────────┐ │
│  │    ESP32 EMAC     │  │   ESP32-P4 EMAC   │  │           External PHY                │ │
│  │  (DWMAC at        │  │  (DWMAC at        │  │  ┌─────────┐  ┌─────────┐  ┌───────┐  │ │
│  │   0x3FF69000)     │  │   0x50084000)     │  │  │LAN8720A │  │ IP101   │  │DP83848│  │ │
│  └───────────────────┘  └───────────────────┘  │  └─────────┘  └─────────┘  └───────┘  │ │
│                                                └───────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

### Port Definitions

#### Driving Ports (Inbound - Application uses these)

| Port | Trait/Type | Description |
|------|------------|-------------|
| `EthDevice` | `smoltcp::phy::Device` | Frame TX/RX for network stacks |
| `PhyDriver` | `phy::PhyDriver` | PHY initialization and status |
| `EthController` | `Emac` methods | Direct driver control |

#### Driven Ports (Outbound - Driver depends on these)

| Port | Current Trait | esp-hal Adapter |
|------|---------------|-----------------|
| `MdioBus` | `hal::MdioBus` | `esp_hal::emac::Mdio` |
| `DelayProvider` | `hal::DelayProvider` | `embassy_time::Delay` |
| `RegisterAccess` | Direct `read_reg`/`write_reg` | PAC registers |
| `ClockProvider` | `ExtRegs` methods | `esp_hal::clock::Clocks` |
| `GpioProvider` | Pin numbers | `esp_hal::gpio::*` types |
| `InterruptController` | Not yet implemented | `esp_hal::interrupt::*` |

---

### Standard Compliance Layers

The codebase is organized by standard compliance to maximize portability:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ Layer 1: IEEE 802.3 Standard (Fully Portable)                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│ • Frame sizes: MTU=1500, MAX_FRAME=1522, ETH_HEADER=14, CRC=4                   │
│ • MDIO/MDC protocol: Clause 22 registers (BMCR, BMSR, PHYIDR, ANAR, ANLPAR)     │
│ • Flow control: 802.3x PAUSE frames, pause time units                          │
│ • Speed/Duplex: 10/100 Mbps, Half/Full duplex                                   │
│ • MAC address: 6-byte format, unicast/multicast bit                            │
│                                                                                 │
│ Files: constants.rs (frame sizes), phy/generic.rs, hal/mdio.rs (phy_reg/*)     │
└─────────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ Layer 2: Synopsys DWMAC IP (Portable across DWMAC SoCs)                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│ • DMA registers: DMABUSMODE, DMASTATUS, DMAOPERATION, descriptor addresses     │
│ • MAC registers: GMACCONFIG, GMACFF, GMACFC, GMACADDR*, GMACMIIADDR            │
│ • Descriptors: Enhanced descriptor format (ATDS=1), OWN bit, FS/LS flags       │
│ • Checksum offload: RX verification, TX insertion modes                        │
│                                                                                 │
│ Files: register/dma.rs, register/mac.rs, descriptor/*.rs, dma.rs               │
│ Other SoCs: STM32H7/F7, Allwinner, Rockchip, Intel Cyclone V                   │
└─────────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ Layer 3: ESP32-Specific (Requires ESP32 hardware)                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│ • Memory map: DMA=0x3FF69000, MAC=0x3FF6A000, EXT=0x3FF69800                    │
│ • Extension registers: EX_PHYINF_CONF, EX_CLK_CTRL, EX_RAM_PD                   │
│ • Clock configuration: RMII internal/external, APLL for 50MHz                  │
│ • GPIO routing: Fixed RMII data pins, configurable SMI pins                    │
│ • RAM power control: power-up sequence for EMAC SRAM                           │
│                                                                                 │
│ Files: register/mod.rs (addresses), register/ext.rs, hal/clock.rs              │
└─────────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ Layer 4: PHY-Specific (Vendor implementations)                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│ • LAN8720A: Microchip/SMSC, PHY ID 0x0007C0Fx, PSCSR for speed indication       │
│ • IP101: IC+, PHY ID 0x02430C54, different vendor registers                     │
│ • DP83848: TI, PHY ID 0x20005C90, PHYSTS for speed/duplex                       │
│                                                                                 │
│ Files: phy/lan8720a.rs, (future) phy/ip101.rs, phy/dp83848.rs                   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

### esp-hal Integration Status

#### Phase 1: embedded-hal Integration ✅ COMPLETE

**Goal**: Use standard embedded-hal traits for ecosystem compatibility.

| Task | Status | Notes |
|------|--------|-------|
| `MdioBus` trait | ✅ Done | Already trait-based |
| `DelayNs` integration | ✅ Done | Required, use esp-hal's `Delay` |
| `PhyDriver` trait | ✅ Done | Generic over `MdioBus` |
| `OutputPin` for PHY reset | ✅ Done | `Lan8720aWithReset<RST: OutputPin>` |
| GPIO documentation | ✅ Done | Fixed pins documented in `hal/gpio.rs` |
| Remove fake configurability | ✅ Done | `SmiPins`/`RmiiPins` removed from public API |

**Current API**:
```rust
use esp_hal::delay::Delay;
use esp32_emac::{Emac, EmacConfig, MdioController};

// Provide esp-hal delay
let mut delay = Delay::new();

// Initialize EMAC
emac.init(EmacConfig::new(), &mut delay)?;

// Use with MDIO
let mut mdio = MdioController::new(&mut delay);
```

#### Phase 2: esp-hal Adapter Crate (Future - Optional)

**Goal**: Create `esp-hal-emac` adapter crate for deeper integration.

**Potential Crate Structure**:
```
esp-hal-emac/
├── Cargo.toml          # Depends on esp-hal + esp32-emac (this crate)
├── src/
│   ├── lib.rs
│   ├── peripheral.rs   # Peripheral<EMAC> ownership wrapper
│   ├── interrupt.rs    # Interrupt handler registration
│   └── async.rs        # Async receive/transmit with embassy
```

**Potential API**:
```rust
use esp_hal::prelude::*;
use esp_hal_emac::{Eth, EthConfig};

#[esp_hal::main]
async fn main(spawner: Spawner) {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    
    // Peripheral ownership pattern
    let eth = Eth::new(
        peripherals.EMAC,
        EthConfig::default()
            .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
            .with_phy::<Lan8720a>(0)
            .with_pins(EthPins {
                tx_en: peripherals.GPIO21,
                txd0: peripherals.GPIO19,
                txd1: peripherals.GPIO22,
                crs_dv: peripherals.GPIO27,
                rxd0: peripherals.GPIO25,
                rxd1: peripherals.GPIO26,
                mdc: peripherals.GPIO23,
                mdio: peripherals.GPIO18,
                ref_clk: peripherals.GPIO0,
            }),
    );
    
    // embassy-net integration
    let (runner, device) = eth.split();
    spawner.spawn(eth_task(runner)).ok();
    
    // Use with embassy-net
    let config = embassy_net::Config::dhcpv4(Default::default());
    let stack = embassy_net::Stack::new(device, config, ...);
}
```

#### Phase 3: Async Support (v0.3 → v0.4)

**Goal**: Add `async` TX/RX with interrupt-driven wakeups.

**Design**:
```rust
// Blocking mode (current)
pub struct Eth<'d, M: Mode = Blocking> { ... }

impl Eth<'d, Blocking> {
    pub fn transmit(&mut self, data: &[u8]) -> Result<usize> { ... }
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize> { ... }
}

// Async mode (Phase 3)
impl Eth<'d, Async> {
    pub async fn transmit(&mut self, data: &[u8]) -> Result<usize> {
        poll_fn(|cx| {
            if self.inner.tx_ready() {
                Poll::Ready(self.inner.transmit(data))
            } else {
                self.tx_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }
    
    pub async fn receive(&mut self, buffer: &mut [u8]) -> Result<usize> {
        poll_fn(|cx| {
            if self.inner.rx_available() {
                Poll::Ready(self.inner.receive(buffer))
            } else {
                self.rx_waker.register(cx.waker());
                Poll::Pending
            }
        }).await
    }
}

// Interrupt handler
#[handler]
fn EMAC_IRQ() {
    let status = Eth::interrupt_status();
    if status.rx_complete {
        RX_WAKER.wake();
    }
    if status.tx_complete {
        TX_WAKER.wake();
    }
    Eth::clear_interrupts();
}
```

#### Phase 4: embassy-net Driver (v0.4 → v1.0)

**Goal**: Implement `embassy_net::Driver` for seamless integration.

```rust
impl<'d> embassy_net::Driver for EthDevice<'d> {
    type RxToken<'a> = EthRxToken<'a> where Self: 'a;
    type TxToken<'a> = EthTxToken<'a> where Self: 'a;
    
    fn receive(&mut self, cx: &mut Context) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if self.eth.rx_available() && self.eth.tx_ready() {
            Some((EthRxToken { eth: &mut self.eth }, EthTxToken { eth: &mut self.eth }))
        } else {
            self.eth.register_waker(cx.waker());
            None
        }
    }
    
    fn link_state(&mut self, cx: &mut Context) -> LinkState {
        if self.phy.is_link_up() {
            LinkState::Up
        } else {
            LinkState::Down
        }
    }
}
```

---

### PHY Driver Architecture

#### Supported PHY Chips

| PHY | OUI | Status | Notes |
|-----|-----|--------|-------|
| LAN8720A | 0x0007C0Fx | ✅ Implemented | Most common with ESP32 |
| IP101 | 0x02430C54 | 🔲 Planned | Alternative option |
| DP83848 | 0x20005C90 | 🔲 Planned | TI option |
| RTL8201 | 0x001CC810 | 🔲 Planned | Realtek option |

#### PHY Trait Design

```rust
/// Core PHY driver trait
pub trait PhyDriver {
    /// PHY address on MDIO bus
    fn address(&self) -> u8;
    
    /// Initialize PHY (reset + configure)
    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()>;
    
    /// Soft reset via BMCR
    fn soft_reset<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()>;
    
    /// Check if link is up (BMSR.LINK_STATUS)
    fn is_link_up<M: MdioBus>(&self, mdio: &mut M) -> Result<bool>;
    
    /// Get negotiated/configured link parameters
    fn link_status<M: MdioBus>(&self, mdio: &mut M) -> Result<Option<LinkStatus>>;
    
    /// Poll for link changes (for state machine)
    fn poll_link<M: MdioBus>(&mut self, mdio: &mut M) -> Result<Option<LinkStatus>>;
    
    /// Enable auto-negotiation
    fn enable_auto_negotiation<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()>;
    
    /// Force specific link parameters
    fn force_link<M: MdioBus>(&mut self, mdio: &mut M, status: LinkStatus) -> Result<()>;
    
    /// Read PHY ID (for identification)
    fn phy_id<M: MdioBus>(&self, mdio: &mut M) -> Result<u32>;
}
```

#### LAN8720A Specifics

The LAN8720A is the most common PHY for ESP32 Ethernet boards. Key features:

| Feature | Register | Notes |
|---------|----------|-------|
| PHY ID | 0x0007C0Fx | Verify before use |
| Speed indication | PSCSR (reg 31) | More reliable than BMCR |
| Energy detect | MCSR (reg 17) | Low-power mode |
| Interrupts | ISR/IMR (29/30) | Link change notification |

```rust
// Vendor-specific register access
impl Lan8720a {
    /// Read speed/duplex from vendor register (more accurate than BMCR)
    pub fn read_speed_indication<M: MdioBus>(&self, mdio: &mut M) -> Result<Option<LinkStatus>> {
        let pscsr = mdio.read(self.addr, reg::PSCSR)?;
        
        if (pscsr & pscsr::AUTODONE) == 0 {
            return Ok(None);
        }
        
        match pscsr & pscsr::HCDSPEED_MASK {
            x if x == pscsr::HCDSPEED_100FD => Ok(Some(LinkStatus::fast_full())),
            x if x == pscsr::HCDSPEED_100HD => Ok(Some(LinkStatus::fast_half())),
            x if x == pscsr::HCDSPEED_10FD => Ok(Some(LinkStatus::slow_full())),
            x if x == pscsr::HCDSPEED_10HD => Ok(Some(LinkStatus::slow_half())),
            _ => Ok(None),
        }
    }
}
```

---

### Module Dependency Graph

```
                    ┌──────────────────┐
                    │     lib.rs       │ ◄── Public API exports
                    └────────┬─────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐   ┌────────────────┐   ┌────────────────┐
│    mac.rs     │   │   smoltcp.rs   │   │    phy/       │
│ (Emac driver) │   │ (Device impl)  │   │ (PhyDriver)   │
└───────┬───────┘   └────────┬───────┘   └───────┬───────┘
        │                    │                   │
        │           ┌────────┘                   │
        ▼           ▼                            ▼
┌───────────────────────────┐           ┌────────────────┐
│        dma.rs             │           │  phy/generic   │
│    (DmaEngine)            │           │ (PhyDriver)    │
└───────────┬───────────────┘           └───────┬────────┘
            │                                   │
            ▼                                   ▼
┌───────────────────────────┐           ┌────────────────┐
│     descriptor/           │           │ phy/lan8720a   │
│  (TxDescriptor, Rx...)    │           │ (concrete PHY) │
└───────────┬───────────────┘           └───────┬────────┘
            │                                   │
            │                   ┌───────────────┘
            ▼                   ▼
┌───────────────────────────────────────────────────────┐
│                       hal/                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌───────┐ │
│  │ mdio.rs  │  │ clock.rs │  │ reset.rs │  │gpio.rs│ │
│  │(MdioBus) │  │(Delay)   │  │          │  │       │ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └───┬───┘ │
└───────┼─────────────┼─────────────┼────────────┼─────┘
        │             │             │            │
        ▼             ▼             ▼            ▼
┌───────────────────────────────────────────────────────┐
│                    register/                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │  dma.rs  │  │  mac.rs  │  │  ext.rs  │            │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘            │
└───────┼─────────────┼─────────────┼──────────────────┘
        │             │             │
        └─────────────┼─────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│              constants.rs + error.rs + config.rs        │
│                    (no external deps)                   │
└─────────────────────────────────────────────────────────┘
```

---

### Testing Strategy

#### Unit Tests (Mock Adapters)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    /// Mock MDIO bus for testing PHY drivers
    struct MockMdio {
        registers: [[u16; 32]; 32],  // 32 PHYs × 32 registers
    }
    
    impl MdioBus for MockMdio {
        fn read(&mut self, phy: u8, reg: u8) -> Result<u16> {
            Ok(self.registers[phy as usize][reg as usize])
        }
        
        fn write(&mut self, phy: u8, reg: u8, val: u16) -> Result<()> {
            self.registers[phy as usize][reg as usize] = val;
            Ok(())
        }
        
        fn is_busy(&self) -> bool { false }
    }
    
    #[test]
    fn test_lan8720a_init() {
        let mut mdio = MockMdio::default();
        // Set PHY ID registers
        mdio.registers[0][2] = 0x0007;  // PHYIDR1
        mdio.registers[0][3] = 0xC0F1;  // PHYIDR2 (rev 1)
        
        let mut phy = Lan8720a::new(0);
        assert!(phy.verify_id(&mut mdio).unwrap());
    }
}
```

#### Integration Tests (Hardware-in-Loop)

```rust
// tests/hardware_test.rs (run on actual ESP32)
#![no_std]
#![no_main]

#[test]
fn test_phy_detection() {
    let mut mdio = MdioController::new(SpinDelay::default_esp32());
    let found = lan8720a::scan_bus(&mut mdio).unwrap();
    assert!(found.iter().any(|x| x.is_some()), "No LAN8720A found on bus");
}

#[test]
fn test_link_negotiation() {
    // Requires Ethernet cable connected
    let mut phy = Lan8720a::new(0);
    phy.init(&mut mdio).unwrap();
    
    // Wait up to 5 seconds for link
    for _ in 0..500 {
        if let Some(link) = phy.poll_link(&mut mdio).unwrap() {
            assert!(matches!(link.speed, Speed::Mbps10 | Speed::Mbps100));
            return;
        }
        delay.delay_ms(10);
    }
    panic!("Link negotiation timeout");
}
```

---

### Migration Guide

#### From Standalone to esp-hal

```rust
// ===== BEFORE (standalone) =====
use esp32_emac::{Emac, EmacConfig, Lan8720a, PhyDriver};
use esp32_emac::hal::{MdioController, SpinDelay};

static mut EMAC: Emac<10, 10, 1600> = Emac::new();

fn main() {
    let emac = unsafe { &mut EMAC };
    
    let config = EmacConfig::new()
        .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56]);
    
    emac.init(config).unwrap();
    
    let mut mdio = MdioController::new(SpinDelay::default_esp32());
    let mut phy = Lan8720a::new(0);
    phy.init(&mut mdio).unwrap();
    
    emac.start().unwrap();
}

// ===== AFTER (esp-hal) =====
use esp_hal::prelude::*;
use esp_hal_emac::{Eth, EthConfig, Lan8720a};

#[esp_hal::main]
async fn main(spawner: Spawner) {
    let p = esp_hal::init(Default::default());
    
    let eth = Eth::new(
        p.EMAC,
        EthConfig::default()
            .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
            .with_phy::<Lan8720a>(0),
        EthPins::new(p.GPIO21, p.GPIO19, p.GPIO22, p.GPIO27, p.GPIO25, p.GPIO26,
                     p.GPIO23, p.GPIO18, p.GPIO0),
    ).await.unwrap();
    
    // Use with embassy-net...
}
```

---

### Roadmap

| Phase | Version | Timeline | Deliverables |
|-------|---------|----------|--------------|
| **Core Driver** | v0.1 | ✅ Complete | EMAC driver, DMA, descriptors, LAN8720A PHY |
| **Trait Abstraction** | v0.2 | Q1 2026 | GPIO/Clock traits, additional PHY drivers |
| **esp-hal Adapter** | v0.3 | Q2 2026 | `esp-hal-emac` crate, peripheral ownership |
| **Async Support** | v0.4 | Q3 2026 | Interrupt-driven TX/RX, wakers |
| **embassy-net** | v1.0 | Q4 2026 | Full embassy-net integration, DHCP example |

### Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `esp-hal` | 1.0+ | Peripheral access, GPIO, clocks |
| `embassy-net` | 0.4+ | TCP/IP stack |
| `embassy-time` | 0.3+ | Async delays, timeouts |
| `smoltcp` | 0.12+ | Alternative TCP/IP stack |
| `defmt` | 0.3+ | Logging (optional) |