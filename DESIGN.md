# ESP32 EMAC Rust Implementation Design Document

## Overview

This document describes the design for a `no_std`, `no_alloc` Rust implementation of the ESP32 Ethernet MAC (EMAC) controller. The implementation targets bare-metal embedded environments without heap allocation, using only static memory.

### Goals

- **`no_std`**: No standard library dependency
- **`no_alloc`**: Zero heap allocations - all memory statically allocated at compile time
- **Safe abstractions**: Leverage Rust's type system for correctness
- **Zero-cost abstractions**: No runtime overhead compared to C implementation
- **Portable**: Support ESP32 and ESP32-P4 variants via feature flags
- **Ecosystem compatible**: Use `embedded-hal` 1.0 traits for HAL interoperability

---

## Hardware Background

The ESP32 EMAC is based on the Synopsys DesignWare MAC (DWMAC) IP core featuring:

- IEEE 802.3 compliant MAC
- MII/RMII interface support
- DMA engine with descriptor-based transfers
- MDIO interface for PHY management
- 10/100 Mbps operation
- Full/half duplex support
- Hardware checksum offload (optional)
- IEEE 1588 PTP timestamps (ESP32-P4 only)

### Memory Map

| Peripheral | Base Address (ESP32) |
|------------|---------------------|
| EMAC DMA   | 0x3FF69000          |
| EMAC MAC   | 0x3FF6A000          |
| EMAC EXT   | 0x3FF69800          |

### RMII Pin Configuration

ESP32 uses fixed internal routing for RMII pins - they cannot be remapped:

| Signal       | GPIO | Direction | Description |
|--------------|------|-----------|-------------|
| TX_EN        | 21   | Output    | Transmit enable |
| TXD0         | 19   | Output    | Transmit data bit 0 |
| TXD1         | 22   | Output    | Transmit data bit 1 |
| RX_DV (CRS)  | 27   | Input     | Receive data valid / carrier sense |
| RXD0         | 25   | Input     | Receive data bit 0 |
| RXD1         | 26   | Input     | Receive data bit 1 |
| REF_CLK      | 0    | In/Out    | 50 MHz reference clock |
| MDIO         | 18   | Bidir     | Management data I/O |
| MDC          | 23   | Output    | Management data clock |

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
│  │ Registers│  │  Clock   │  │   MDIO   │  │  Descriptors     │ │
│  │ (DMA/MAC)│  │  Config  │  │ Control  │  │  (TX/RX rings)   │ │
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
├── constants.rs           # PHY and EMAC constants
│
├── register/              # Memory-mapped register definitions
│   ├── mod.rs             # Register block abstractions
│   ├── dma.rs             # DMA controller registers
│   ├── mac.rs             # MAC core registers
│   └── ext.rs             # Clock/GPIO extension registers
│
├── descriptor/            # DMA descriptor definitions
│   ├── mod.rs             # Shared descriptor traits
│   ├── tx.rs              # Transmit descriptors
│   └── rx.rs              # Receive descriptors
│
├── hal/                   # Hardware abstraction
│   ├── mod.rs             # HAL exports
│   ├── clock.rs           # Clock tree configuration
│   ├── gpio.rs            # Pin documentation (pins are fixed)
│   ├── mdio.rs            # PHY register access via MDIO/SMI
│   └── reset.rs           # Reset controller
│
├── phy/                   # PHY drivers
│   ├── mod.rs             # Phy trait definition
│   ├── generic.rs         # Generic PHY base implementation
│   └── lan8720a.rs        # LAN8720A PHY driver
│
├── dma.rs                 # DMA engine (buffer management)
├── mac.rs                 # MAC driver implementation
├── smoltcp.rs             # smoltcp Device trait integration
└── sync.rs                # SharedEmac (critical-section wrapper)
```

### File Sizes (Current)

| File | Lines | Description |
|------|-------|-------------|
| `mac.rs` | 1173 | Main driver implementation |
| `register/mac.rs` | 926 | MAC register definitions |
| `dma.rs` | 751 | DMA engine |
| `phy/lan8720a.rs` | 714 | LAN8720A PHY driver |
| `config.rs` | 442 | Configuration structures |
| `register/dma.rs` | 428 | DMA register definitions |
| `hal/mdio.rs` | 421 | MDIO controller |
| `descriptor/rx.rs` | 419 | RX descriptor |
| `descriptor/tx.rs` | 351 | TX descriptor |
| `phy/generic.rs` | 321 | Generic PHY base |
| `smoltcp.rs` | 276 | smoltcp integration |
| `sync.rs` | 233 | SharedEmac wrapper |
| `hal/reset.rs` | 206 | Reset controller |
| `error.rs` | 209 | Error types |
| `register/ext.rs` | 190 | Extension registers |
| `lib.rs` | 191 | Crate root |
| `hal/clock.rs` | 174 | Clock configuration |
| `register/mod.rs` | 169 | Register module |
| `constants.rs` | 151 | Constants |
| `descriptor/mod.rs` | 147 | Descriptor traits |
| **Total** | **~8k** | Core implementation |

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
    rx_descriptors: [RxDescriptor; RX_BUFS],
    tx_descriptors: [TxDescriptor; TX_BUFS],
    rx_buffers: [[u8; BUF_SIZE]; RX_BUFS],
    tx_buffers: [[u8; BUF_SIZE]; TX_BUFS],
    state: EmacState,
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

### Link Section Placement

```rust
#[link_section = ".dram1.emac"]
static mut EMAC_INSTANCE: MaybeUninit<Emac<10, 10, 1600>> = MaybeUninit::uninit();
```

---

## Register Definitions

### Design Principles

1. **Volatile access**: All register access through volatile read/write
2. **Type safety**: Bitfields represented as typed structs
3. **Zero-cost**: Inline functions compile to direct memory access

### Register Blocks

The driver defines three register blocks:

| Block | Base Address | Module | Purpose |
|-------|-------------|--------|---------|
| DMA | 0x3FF69000 | `register/dma.rs` | DMA controller |
| MAC | 0x3FF6A000 | `register/mac.rs` | MAC core |
| EXT | 0x3FF69800 | `register/ext.rs` | Clock/GPIO extensions |

### Key Registers

**DMA Registers:**
- `DMABUSMODE` (0x00): Bus mode configuration, software reset
- `DMATXPOLLDEMAND` (0x04): Transmit poll demand
- `DMARXPOLLDEMAND` (0x08): Receive poll demand
- `DMARXDESCLISTADDR` (0x0C): RX descriptor list base
- `DMATXDESCLISTADDR` (0x10): TX descriptor list base
- `DMAOPMODE` (0x18): Operation mode (start TX/RX)
- `DMAIN_EN` (0x1C): Interrupt enable
- `DMAMISSEDFR` (0x20): Missed frames counter

**MAC Registers:**
- `MACCFG` (0x00): MAC configuration (speed, duplex)
- `MACFFR` (0x04): Frame filter register
- `MACHTHI`/`MACHTLO` (0x08/0x0C): Hash table
- `GMIIADDR` (0x10): MDIO address register
- `GMIIDATA` (0x14): MDIO data register
- `MACFCR` (0x18): Flow control
- `MACADDR0HI`/`MACADDR0LO`: Primary MAC address

---

## DMA Descriptors

### Transmit Descriptor (Enhanced Format)

```
┌─────────────────────────────────────────────────────────────────┐
│ TDES0 (32 bits): Control/Status                                 │
├─────────────────────────────────────────────────────────────────┤
│ TDES1 (32 bits): Buffer sizes, VLAN tag                         │
├─────────────────────────────────────────────────────────────────┤
│ TDES2 (32 bits): Buffer 1 address (packet data)                 │
├─────────────────────────────────────────────────────────────────┤
│ TDES3 (32 bits): Buffer 2 address / Next descriptor             │
├─────────────────────────────────────────────────────────────────┤
│ TDES4-7 (reserved for timestamps)                               │
└─────────────────────────────────────────────────────────────────┘
```

Key TDES0 bits:
- Bit 31: OWN - DMA owns descriptor
- Bit 30: IC - Interrupt on completion
- Bit 29: LS - Last segment
- Bit 28: FS - First segment
- Bits 23-22: Checksum insertion mode

### Receive Descriptor (Enhanced Format)

```
┌─────────────────────────────────────────────────────────────────┐
│ RDES0 (32 bits): Status                                         │
├─────────────────────────────────────────────────────────────────┤
│ RDES1 (32 bits): Control, buffer sizes                          │
├─────────────────────────────────────────────────────────────────┤
│ RDES2 (32 bits): Buffer 1 address                               │
├─────────────────────────────────────────────────────────────────┤
│ RDES3 (32 bits): Buffer 2 address / Next descriptor             │
├─────────────────────────────────────────────────────────────────┤
│ RDES4-7 (reserved for timestamps)                               │
└─────────────────────────────────────────────────────────────────┘
```

Key RDES0 bits:
- Bit 31: OWN - DMA owns descriptor
- Bit 15: ES - Error summary
- Bits 0-13: FL - Frame length

---

## DMA Engine

The DMA engine manages circular descriptor rings for TX and RX.

### Ring Structure

```rust
pub struct DmaEngine<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    rx_ring: RxRing<RX_BUFS, BUF_SIZE>,
    tx_ring: TxRing<TX_BUFS, BUF_SIZE>,
}
```

### Transmit Flow

1. Application calls `transmit(data)`
2. Driver finds next available TX descriptor (OWN=0)
3. Copies data to TX buffer
4. Sets FS, LS bits for single-frame packet
5. Sets OWN=1 to transfer ownership to DMA
6. Writes to TX poll demand register
7. DMA transmits packet, clears OWN when complete

### Receive Flow

1. DMA receives packet into RX buffer
2. Clears OWN bit, sets frame length in status
3. Application calls `receive()`
4. Driver reads frame length, copies data to application buffer
5. Sets OWN=1 to return descriptor to DMA

---

## MAC Driver

### Configuration

```rust
pub struct EmacConfig {
    pub phy_interface: PhyInterface,
    pub rmii_clock_mode: RmiiClockMode,
    pub mac_address: [u8; 6],
    pub speed: Speed,
    pub duplex: Duplex,
    pub rx_checksum_offload: bool,
    pub tx_checksum_mode: TxChecksumMode,
    pub dma_burst_length: DmaBurstLength,
    pub flow_control: FlowControlConfig,
}
```

### Driver State Machine

```
┌───────────────┐     init()     ┌───────────────┐
│   Created     │ ──────────────▶│  Initialized  │
└───────────────┘                └───────────────┘
                                        │
                                 start()│
                                        ▼
                                 ┌───────────────┐
                           ┌─────│   Running     │◀────┐
                           │     └───────────────┘     │
                      stop()│            │        start()
                           │            │             │
                           ▼      handle_interrupt()  │
                    ┌───────────────┐                 │
                    │   Stopped     │─────────────────┘
                    └───────────────┘
```

### API

```rust
impl<const RX, const TX, const BUF> Emac<RX, TX, BUF> {
    // Lifecycle
    pub const fn new() -> Self;
    pub fn init<D: DelayNs>(&mut self, config: EmacConfig, delay: &mut D) -> Result<()>;
    pub fn start(&mut self) -> Result<()>;
    pub fn stop(&mut self) -> Result<()>;
    
    // Data transfer
    pub fn transmit(&mut self, data: &[u8]) -> Result<()>;
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize>;
    pub fn can_transmit(&self) -> bool;
    pub fn can_receive(&self) -> bool;
    
    // PHY access
    pub fn read_phy_reg<D: DelayNs>(&mut self, reg: u8, delay: &mut D) -> Result<u16>;
    pub fn write_phy_reg<D: DelayNs>(&mut self, reg: u8, val: u16, delay: &mut D) -> Result<()>;
    
    // Interrupts
    pub fn read_interrupt_status(&self) -> InterruptStatus;
    pub fn clear_interrupts(&mut self, status: InterruptStatus);
    pub fn enable_interrupts(&mut self, mask: InterruptMask);
    
    // MAC address & filtering
    pub fn set_mac_address(&mut self, addr: [u8; 6]);
    pub fn get_mac_address(&self) -> [u8; 6];
    pub fn add_mac_filter(&mut self, slot: usize, addr: [u8; 6]) -> Result<()>;
    pub fn add_hash_filter(&mut self, addr: [u8; 6]);
    pub fn set_promiscuous(&mut self, enabled: bool);
    pub fn set_pass_all_multicast(&mut self, enabled: bool);
    
    // Flow control
    pub fn send_pause_frame(&mut self, pause_time: u16);
}
```

---

## PHY Driver

### Architecture

The driver uses a trait-based PHY abstraction:

```rust
pub trait Phy<D: DelayNs> {
    fn init(&mut self, mdio: &mut MdioController<D>) -> Result<()>;
    fn get_link_status(&self, mdio: &mut MdioController<D>) -> Result<LinkStatus>;
    fn auto_negotiate(&mut self, mdio: &mut MdioController<D>) -> Result<AutoNegResult>;
}

pub struct LinkStatus {
    pub link_up: bool,
    pub speed: Speed,
    pub duplex: Duplex,
}
```

### LAN8720A Driver

The LAN8720A is a common low-cost 10/100 PHY. The driver supports:

- Auto-negotiation
- Link status polling
- Speed/duplex detection
- Optional hardware reset pin (via `embedded_hal::digital::OutputPin`)

```rust
// Without reset pin
let phy = Lan8720a::new(phy_address);

// With reset pin (esp-hal integration)
use embedded_hal::digital::OutputPin;
let reset_pin: impl OutputPin = gpio0.into_output();
let phy = Lan8720aWithReset::new(phy_address, reset_pin);
phy.hardware_reset(&mut delay)?;
```

### Supported PHYs

| PHY | Status | Module |
|-----|--------|--------|
| LAN8720A | ✅ Complete | `phy/lan8720a.rs` |
| IP101 | 🔲 Planned | - |
| RTL8201 | 🔲 Planned | - |
| DP83848 | 🔲 Planned | - |

---

## HAL Layer

### Clock Configuration

```rust
pub struct ClockController {
    state: ClockState,
}

impl ClockController {
    pub fn configure(&mut self, mode: RmiiClockMode);
}
```

Clock modes:
- `RmiiClockMode::OutputGpio0`: ESP32 generates 50 MHz on GPIO0
- `RmiiClockMode::InputGpio0`: External 50 MHz on GPIO0

### MDIO Controller

The MDIO controller provides PHY register access using `embedded_hal::delay::DelayNs`:

```rust
pub struct MdioController<D: DelayNs> {
    delay: D,
    phy_address: u8,
}

impl<D: DelayNs> MdioController<D> {
    pub fn read_register(&mut self, register: u8) -> Result<u16>;
    pub fn write_register(&mut self, register: u8, value: u16) -> Result<()>;
}
```

### Reset Controller

```rust
pub struct ResetController<D: DelayNs> {
    delay: D,
}

impl<D: DelayNs> ResetController<D> {
    pub fn reset_emac(&mut self) -> Result<()>;
    pub fn reset_dma(&mut self) -> Result<()>;
}
```

---

## embedded-hal Integration

The driver uses `embedded-hal` 1.0 traits as a **required** dependency (not optional):

### Cargo.toml

```toml
[dependencies]
embedded-hal = { version = "1.0" }
```

### Traits Used

| Trait | Module | Usage |
|-------|--------|-------|
| `embedded_hal::delay::DelayNs` | `hal/mdio.rs`, `hal/reset.rs`, `mac.rs` | Timing for MDIO operations, resets |
| `embedded_hal::digital::OutputPin` | `phy/lan8720a.rs` | PHY hardware reset pin |

### Why embedded-hal is Required

1. **Ecosystem compatibility**: Works with any HAL that implements embedded-hal (esp-hal, embassy, etc.)
2. **No custom delay types**: Removed all internal delay implementations
3. **Simpler API**: Users provide their HAL's delay; no trait negotiation

### Example Usage

```rust
use embedded_hal::delay::DelayNs;
use esp_hal::delay::Delay;

let mut delay = Delay::new();
let emac = unsafe { &mut EMAC };
emac.init(config, &mut delay)?;

// PHY operations
let link = emac.read_phy_reg(0x01, &mut delay)?;
```

---

## smoltcp Integration

The `smoltcp` feature enables the `smoltcp::phy::Device` trait implementation:

### Feature Flag

```toml
[features]
smoltcp = ["dep:smoltcp"]

[dependencies]
smoltcp = { version = "0.12", default-features = false, optional = true }
```

### Implementation

```rust
#[cfg(feature = "smoltcp")]
impl<const RX, const TX, const BUF> smoltcp::phy::Device for Emac<RX, TX, BUF> {
    type RxToken<'a> = EmacRxToken<'a, RX, TX, BUF>;
    type TxToken<'a> = EmacTxToken<'a, RX, TX, BUF>;
    
    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)>;
    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>>;
    fn capabilities(&self) -> DeviceCapabilities;
}
```

### Usage with smoltcp

```rust
use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::wire::{IpAddress, IpCidr};

let emac = unsafe { &mut EMAC };
emac.init(config, &mut delay)?;
emac.start()?;

let mut iface = Interface::new(Config::new(mac_addr.into()), emac, Instant::ZERO);
iface.update_ip_addrs(|addrs| {
    addrs.push(IpCidr::new(IpAddress::v4(192, 168, 1, 100), 24)).unwrap();
});

// Use with sockets...
```

---

## Error Handling

Errors are represented as a `Copy` enum with no heap allocation:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    // Initialization
    NotInitialized,
    AlreadyInitialized,
    InitializationFailed,
    
    // Data transfer
    BufferTooSmall,
    NoBuffersAvailable,
    TransmitError,
    ReceiveError,
    
    // PHY
    PhyError,
    PhyNotFound,
    MdioTimeout,
    LinkDown,
    AutoNegotiationFailed,
    
    // Configuration
    InvalidConfiguration,
    InvalidPhyAddress,
    InvalidMacAddress,
    InvalidSlot,
}

impl Error {
    pub const fn as_str(&self) -> &'static str;
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_str())
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

# Optional integrations
smoltcp = ["dep:smoltcp"]           # smoltcp network stack integration
critical-section = ["dep:critical-section"]  # ISR-safe SharedEmac wrapper
esp-hal = ["dep:esp-hal", "critical-section"]  # esp-hal ergonomic integration
async = ["critical-section"]        # Async/await support with wakers

# Development
defmt = ["dep:defmt"]  # defmt logging support

[dependencies]
embedded-hal = { version = "1.0" }  # REQUIRED
smoltcp = { version = "0.12", default-features = false, optional = true }
critical-section = { version = "1.2", optional = true }
esp-hal = { version = "1.0", optional = true, default-features = false }
defmt = { version = "0.3", optional = true }
```

Note: ESP32-P4 support is reserved for future work and is intentionally out of
scope for this release.

### esp-hal Feature

Enables ergonomic integration with esp-hal's interrupt system and types:

```rust
use ph_esp32_mac::{Emac, EmacConfig, SharedEmac};
use ph_esp32_mac::esp_hal::{EmacExt, Priority, Delay};

static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();

// Define interrupt handler using esp-hal's #[handler] macro
#[esp_hal::handler(priority = Priority::Priority1)]
fn emac_handler() {
    EMAC.with(|emac| {
        let status = emac.read_interrupt_status();
        emac.clear_interrupts(status);
        
        if status.rx_complete() {
            // Wake RX task...
        }
    });
}

fn main() {
    let mut delay = Delay::new();
    
    EMAC.with(|emac| {
        emac.init(config, &mut delay).unwrap();
        emac.enable_emac_interrupt(emac_handler);  // ← esp-hal integration
        emac.start().unwrap();
    });
}
```

The `esp-hal` feature provides:
- `EmacExt` trait: `enable_emac_interrupt()`, `disable_emac_interrupt()`
- `emac_isr!` macro: Convenience macro for defining ISR handlers
- `EspHalEmac`: Wrapper for future peripheral ownership integration
- Re-exports: `Delay`, `Priority`, `InterruptHandler`, `Interrupt`

### async Feature

Enables async/await support using interrupt-driven wakers:

```rust
use ph_esp32_mac::{Emac, EmacConfig, AsyncEmacExt};
use ph_esp32_mac::asynch::async_interrupt_handler;

static mut EMAC: Emac<10, 10, 1600> = Emac::new();

// Interrupt handler must call async_interrupt_handler()
#[interrupt]
fn ETH_MAC() {
    async_interrupt_handler();
}

async fn ethernet_task() {
    let emac = unsafe { &mut EMAC };
    let mut rx_buf = [0u8; 1600];
    
    loop {
        // Async receive - yields to executor when no frames available
        let len = emac.receive_async(&mut rx_buf).await.unwrap();
        
        // Process the frame...
        let response = process(&rx_buf[..len]);
        
        // Async transmit - yields if TX buffers are full
        let _ = emac.transmit_async(&response).await.unwrap();
    }
}
```

The `async` feature provides:
- `AsyncEmacExt` trait: `receive_async()`, `transmit_async()`, `wait_for_error()`
- `async_interrupt_handler()`: ISR handler that wakes async tasks
- `RX_WAKER`, `TX_WAKER`, `ERR_WAKER`: Static wakers for each event type
- `RxFuture`, `TxFuture`, `ErrorFuture`: Future types for manual use

When combined with `esp-hal`:

```rust
use ph_esp32_mac::esp_hal::{emac_isr, Priority, EmacExt};

emac_isr!(ASYNC_HANDLER, Priority::Priority1, {
    ph_esp32_mac::asynch::async_interrupt_handler();
});

// In main:
emac.enable_emac_interrupt(ASYNC_HANDLER);
```

### critical-section Feature

Enables `SharedEmac` for interrupt-safe access:

```rust
use ph_esp32_mac::{SharedEmac, EmacConfig};

static EMAC: SharedEmac<10, 10, 1600> = SharedEmac::new();

fn main() {
    EMAC.with(|emac| {
        emac.init(config, &mut delay).unwrap();
        emac.start().unwrap();
    });
}

#[interrupt]
fn EMAC_IRQ() {
    EMAC.with(|emac| {
        let status = emac.read_interrupt_status();
        emac.clear_interrupts(status);
    });
}
```

---

## Testing Strategy

### Unit Tests

- Descriptor layout verification (size, alignment)
- State machine transitions
- Configuration validation
- Error handling

### Integration Tests (Hardware-in-Loop)

- Loopback mode testing
- Real PHY communication
- Packet TX/RX verification
- Stress testing with traffic generators

### Test Matrix

| Test Type | Environment | Status |
|-----------|-------------|--------|
| Unit tests | Host (`cargo test`) | 🔲 Planned |
| Loopback | ESP32 hardware | 🔲 Planned |
| PHY tests | ESP32 + LAN8720A | 🔲 Planned |
| smoltcp integration | ESP32 hardware | 🔲 Planned |

---

## Implementation Status

### Completed Features (ESP-IDF Parity)

| Feature | Module | Notes |
|---------|--------|-------|
| Basic TX/RX | `dma.rs`, `mac.rs` | Full implementation |
| DMA descriptor rings | `dma.rs` | Enhanced descriptor format |
| MDIO PHY read/write | `mac.rs` | Via `MdioController` |
| MAC address set/get | `mac.rs` | Primary address |
| Speed configuration | `mac.rs` | 10/100 Mbps |
| Duplex configuration | `mac.rs` | Half/Full |
| Promiscuous mode | `mac.rs` | `set_promiscuous()` |
| Pass all multicast | `mac.rs` | `set_pass_all_multicast()` |
| Interrupt handling | `mac.rs` | `InterruptStatus`, `handle_interrupt()` |
| MII/RMII interface | `config.rs` | RMII default |
| RMII clock modes | `config.rs` | Internal/external |
| RX checksum offload | `config.rs` | Hardware IP/TCP/UDP |
| TX checksum insertion | `descriptor.rs` | 4 modes |
| DMA burst length | `config.rs` | 1-32 beats |
| Flow control (PAUSE) | `mac.rs` | Software-driven |
| MAC address filtering | `mac.rs` | Up to 4 filter slots |
| Hash table filtering | `mac.rs` | 64-bit CRC-32 hash |
| VLAN tag filtering | `mac.rs` | 802.1Q C-VLAN/S-VLAN |
| smoltcp integration | `smoltcp.rs` | `Device` trait |
| embedded-hal delay | `hal/*.rs` | DelayNs required |
| embedded-hal GPIO | `phy/lan8720a.rs` | OutputPin for reset |

### Remaining Work

| Feature | Priority | Effort |
|---------|----------|--------|
| Unit tests | High | Medium |
| Hardware integration tests | High | High |
| Additional PHY drivers | Medium | Low each |
| Async/Embassy support | Medium | High |
| ESP32-P4 support | Low | Medium |

---

## Future Work

### esp-hal Adapter Crate (Optional)

A separate `esp-hal-emac` crate could provide tighter integration:

```rust
// Future: esp-hal-emac crate
use esp_hal::peripherals::EMAC;
use esp_hal::gpio::GpioPin;
use esp_hal_emac::EspHalEmac;

let emac = EspHalEmac::new(
    peripherals.EMAC,
    peripherals.GPIO21,  // TX_EN
    peripherals.GPIO19,  // TXD0
    // ... etc
    phy_config,
);
```

### Async Support

Embassy integration with interrupt-driven wakers:

```rust
// Future: async API
impl<const RX, const TX, const BUF> Emac<RX, TX, BUF> {
    pub async fn transmit_async(&mut self, data: &[u8]) -> Result<()>;
    pub async fn receive_async(&mut self, buffer: &mut [u8]) -> Result<usize>;
}
```

### embassy-net Integration

Full embassy-net driver:

```rust
// Future: embassy-net driver
use embassy_net::{Stack, StackResources};

let (device, runner) = embassy_emac::new(peripherals.EMAC, ...);
let stack = Stack::new(device, config, resources);
spawner.spawn(net_task(stack)).unwrap();
```

---

## Roadmap

| Phase | Status | Deliverables |
|-------|--------|--------------|
| Core Driver | ✅ Complete | EMAC driver, DMA, descriptors |
| PHY Driver | ✅ Complete | LAN8720A driver, Phy trait |
| embedded-hal | ✅ Complete | DelayNs, OutputPin integration |
| smoltcp | ✅ Complete | Device trait implementation |
| Flow Control | ✅ Complete | PAUSE frames, water marks |
| Filtering | ✅ Complete | MAC, hash, VLAN filters |
| Async | ✅ Complete | Feature-gated async module |
| Testing | ✅ Complete | 167 unit tests passing |
| ESP32-P4 | 🔲 Future | PTP, alternate descriptors |

---

## References

- [ESP32 Technical Reference Manual](https://www.espressif.com/sites/default/files/documentation/esp32_technical_reference_manual_en.pdf) - Chapter 10: Ethernet MAC
- [Synopsys DesignWare MAC Databook](https://www.synopsys.com/) - DWC Ethernet MAC programming guide
- [LAN8720A Datasheet](https://www.microchip.com/wwwproducts/en/LAN8720A)
- [embedded-hal 1.0](https://docs.rs/embedded-hal/1.0)
- [smoltcp Documentation](https://docs.rs/smoltcp/0.12)
