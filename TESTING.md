# Testing Plan

This document outlines the testing strategy for the ESP32 EMAC driver, covering both host-based unit tests and hardware integration tests.

---

## Implementation Status

| Module | Tests | Status | Last Updated |
|--------|-------|--------|--------------|
| `descriptor/rx.rs` | 13 | ✅ Implemented | 2026-02-03 |
| `descriptor/tx.rs` | 17 | ✅ Implemented | 2026-02-03 |
| `config.rs` | 19 | ✅ Implemented | 2026-02-03 |
| `mac.rs` (InterruptStatus) | 28 | ✅ Implemented | 2026-02-03 |
| `error.rs` | 22 | ✅ Implemented | 2026-02-03 |
| `hal/mdio.rs` | 14 | ✅ Implemented | 2026-02-03 |
| `phy/lan8720a.rs` | 46 | ✅ Implemented | 2026-02-03 |
| `dma.rs` | 2 | ✅ Implemented | 2026-02-03 |
| `test_utils.rs` | 5 | ✅ Implemented | 2026-02-03 |
| `constants.rs` | 29 | ✅ Implemented | 2026-02-03 |
| `asynch.rs` | 12 | ✅ Implemented | 2026-02-03 |
| `smoltcp.rs` | 9 | ✅ Implemented | 2026-02-03 |
| `sync.rs` | 11 | ✅ Implemented | 2026-02-03 |
| `descriptor/mod.rs` | 1 | ✅ Implemented | 2026-02-03 |
| **Total** | **229** | ✅ All Passing | 2026-02-03 |

### Code Coverage

| Metric | Value | Notes |
|--------|-------|-------|
| Region Coverage | 60.26% | Functions and branches |
| Line Coverage | 55.66% | Executable lines |
| 100% Coverage | `constants.rs`, `sync.rs` | Fully tested modules |
| High Coverage | `error.rs` (98%), `config.rs` (93%) | Well-tested modules |

---

## Table of Contents

1. [Testing Philosophy](#testing-philosophy)
2. [Unit Testing (Host)](#unit-testing-host)
3. [Integration Testing (Hardware)](#integration-testing-hardware)
4. [Test Infrastructure](#test-infrastructure)
5. [Coverage Goals](#coverage-goals)
6. [Running Tests](#running-tests)

---

## Testing Philosophy

### Guiding Principles

1. **Test What You Can on Host** - Maximize unit test coverage for logic that doesn't require hardware
2. **Mock Hardware Interactions** - Use traits and dependency injection for testable code
3. **Integration Tests for Hardware** - Real hardware tests for DMA, PHY, and timing-sensitive code
4. **No Alloc in Production** - Tests may use `std` for mocks, but production code remains `no_std`/`no_alloc`

### Test Pyramid

```text
                    ┌─────────────────┐
                    │   End-to-End    │  ← Real network traffic tests
                    │   (Hardware)    │
                 ┌──┴─────────────────┴──┐
                 │   Integration Tests   │  ← ESP32 + PHY + loopback
                 │      (Hardware)       │
              ┌──┴───────────────────────┴──┐
              │        Unit Tests           │  ← Host-based, fast (229 tests)
              │          (Host)             │
              └─────────────────────────────┘
```

---

## Unit Testing (Host)

Host-based unit tests run on the development machine using `cargo test`. They test logic that doesn't require actual ESP32 hardware.

### 1. Descriptor Tests (`descriptor/`)

**Status:** ✅ **IMPLEMENTED** (30 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **RxDescriptor Layout** | Size, alignment, const size | ✅ |
| **TxDescriptor Layout** | Size, alignment, const size | ✅ |
| **Ownership Bits** | `is_owned()`, `set_owned()`, `clear_owned()`, bit position | ✅ |
| **RX Status Parsing** | Frame length, first/last flags, error detection | ✅ |
| **TX Control Bits** | `prepare()`, `prepare_and_submit()`, checksum modes | ✅ |
| **Buffer Operations** | Buffer size extraction, recycle, reset | ✅ |

---

### 2. Configuration Tests (`config.rs`)

**Status:** ✅ **IMPLEMENTED** (19 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **EmacConfig Builder** | Defaults, chaining, all setters | ✅ |
| **Enum Conversions** | `DmaBurstLen::to_pbl()`, default values | ✅ |
| **MAC Address Filter** | `new()`, `source()`, `with_mask()` | ✅ |
| **Flow Control** | Default config, water mark settings | ✅ |

---

### 3. Error Type Tests (`error.rs`)

**Status:** ✅ **IMPLEMENTED** (22 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **ConfigError** | `as_str()`, Display, PartialEq, Clone | ✅ |
| **DmaError** | `as_str()`, Display, PartialEq | ✅ |
| **IoError** | `as_str()`, Display, PartialEq | ✅ |
| **Unified Error** | `From` conversions, Display with domain prefix | ✅ |
| **Result Types** | `Result<T>`, `ConfigResult<T>`, `DmaResult<T>`, `IoResult<T>` | ✅ |

---

### 4. InterruptStatus Tests (`mac.rs`)

**Status:** ✅ **IMPLEMENTED** (28 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **from_raw()** | All interrupt bits, zero value, all-bits-set | ✅ |
| **to_raw()** | Round-trip single bits, all bits, zero | ✅ |
| **any()** | False when zero, true for TX/RX/errors, ignores summary bits | ✅ |
| **has_error()** | Detects underflow, overflow, fatal bus error | ✅ |
| **Default** | Equals zero state | ✅ |

---

### 5. MDIO/PHY Register Tests (`hal/mdio.rs`)

**Status:** ✅ **IMPLEMENTED** (14 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **Clock Divider** | `from_sys_clock_hz()` all ranges, `to_reg_value()`, default | ✅ |
| **BMSR Parsing** | Link status, auto-neg complete, capability bits | ✅ |
| **ANLPAR Parsing** | Speed/duplex capabilities, pause capability | ✅ |
| **BMCR Bits** | Reset, speed/duplex, auto-neg enable/restart | ✅ |
| **PhyStatus** | Default values | ✅ |

---

### 6. PHY Driver Tests (`phy/lan8720a.rs`)

**Status:** ✅ **IMPLEMENTED** (46 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **PHY ID** | ID check, verify_id(), phy_id(), revision() | ✅ |
| **Initialization** | Soft reset, disable EDPWRDOWN, enable AN, reset state | ✅ |
| **Soft Reset** | Writes RESET bit, waits for clear | ✅ |
| **Link Status** | is_link_up(), link_status() when up/down | ✅ |
| **Poll Link** | Transition detection, link flap handling | ✅ |
| **Auto-Negotiation** | ANAR writes, AN restart, completion detection | ✅ |
| **Force Link** | Disable AN, all speed/duplex combinations | ✅ |
| **Speed Indication** | All 4 speed/duplex combinations from PSCSR | ✅ |
| **Capabilities** | Read from BMSR, link partner abilities | ✅ |
| **Vendor Features** | EDPWRDOWN, interrupts, symbol errors, advertisement | ✅ |
| **PHY Address** | Address getter, operations use correct address | ✅ |

---

### 7. DMA Tests (`dma.rs`)

**Status:** ✅ **IMPLEMENTED** (2 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **Descriptor Ring** | Ring index advance/wrap | ✅ |
| **Memory Usage** | Memory size calculations | ✅ |

---

### 8. Test Utilities (`test_utils.rs`)

**Status:** ✅ **IMPLEMENTED** (5 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **MockMdioBus** | Read/write, multiple PHYs, LAN8720A setup, link simulation | ✅ |
| **MockDelay** | Delay tracking | ✅ |

---

### 9. Constants Tests (`constants.rs`)

**Status:** ✅ **IMPLEMENTED** (29 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **Frame Sizes** | MTU, max frame, min frame, header sizes | ✅ |
| **Timing Constants** | Flush timeout, soft reset timeout, MII busy timeout | ✅ |
| **Clock Frequencies** | RMII 50MHz, MII clocks, MDC max | ✅ |
| **MAC Address** | Default MAC validation (locally administered, unicast) | ✅ |
| **DMA States** | Shift positions, masks, no overlap | ✅ |
| **Buffer Defaults** | Buffer sizes, counts, flow control water marks | ✅ |

---

### 10. Async Tests (`asynch.rs`)

**Status:** ✅ **IMPLEMENTED** (12 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **AtomicWaker** | `new()`, `register()`, `wake()`, `take()` | ✅ |
| **Waker Behavior** | Overwrite on re-register, wake clears, double wake | ✅ |
| **Static Wakers** | `TX_WAKER`, `RX_WAKER`, `ERR_WAKER` independence | ✅ |
| **Async State** | `reset_async_state()` wakes all pending | ✅ |

---

### 11. smoltcp Integration Tests (`smoltcp.rs`)

**Status:** ✅ **IMPLEMENTED** (9 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **Medium** | `Medium::Ethernet` validation | ✅ |
| **MTU** | MTU constant matches Ethernet standard | ✅ |
| **Checksum** | All `Checksum` variants constructable | ✅ |
| **ChecksumCapabilities** | Default construction, field access | ✅ |
| **DeviceCapabilities** | Default values, medium, max_burst_size | ✅ |

---

### 12. Sync Wrapper Tests (`sync.rs`)

**Status:** ✅ **IMPLEMENTED** (11 tests)

| Test Category | Tests | Status |
|---------------|-------|--------|
| **Construction** | `new()`, `Default` trait | ✅ |
| **Access Patterns** | `with()`, `try_with()`, nested calls | ✅ |
| **Return Values** | Closure return value propagation | ✅ |
| **Type Aliases** | `SharedEmacSmall`, `SharedEmacLarge` | ✅ |
| **Static Allocation** | Static cell pattern with `RefCell` | ✅ |

---

### Future Unit Tests (Not Yet Implemented)

The following unit tests are planned but require additional infrastructure:

#### EMAC State Machine (`mac.rs`)

Requires mock register layer for hardware register access:

| Test Category | Test Cases | Priority |
|---------------|------------|----------|
| **State Transitions** | Uninitialized→Stopped→Running→Stopped | Medium |
| **Invalid Transitions** | Error handling for invalid operations | Medium |

---

## Integration Testing (Hardware)

Integration tests run on actual ESP32 hardware and test the complete driver stack.

### Hardware Requirements

| Item | Specification | Notes |
|------|---------------|-------|
| ESP32 Dev Board | ESP32-Ethernet-Kit or ESP32-PoE | Must have RMII PHY |
| PHY Chip | LAN8720A | Tested and supported |
| Network Switch | Any managed switch | For traffic monitoring |
| Test PC | Linux/Windows/Mac | With Ethernet port |

### Test Categories

| Category | Tests | Status |
|----------|-------|--------|
| Hardware Initialization | EMAC reset, clock config, DMA init, MAC address | 🔲 Planned |
| PHY Communication | MDIO read/write, soft reset, auto-negotiation | 🔲 Planned |
| Loopback Tests | PHY loopback TX→RX, various frame sizes | 🔲 Planned |
| Real Network Tests | ARP, ICMP ping, TCP connection | 🔲 Planned |
| Interrupt Tests | RX/TX interrupt firing, async waker integration | 🔲 Planned |
| Error Handling | Buffer overflow, CRC errors, cable disconnect | 🔲 Planned |
| Performance Tests | Throughput, latency measurements | 🔲 Planned |

---

## Test Infrastructure

### Mock MDIO Bus (`test_utils.rs`)

**Status:** ✅ **IMPLEMENTED**

The `MockMdioBus` provides a complete mock implementation for testing PHY drivers without hardware:

```rust
use crate::test_utils::MockMdioBus;

#[test]
fn test_phy_with_mock() {
    let mut mdio = MockMdioBus::new();
    
    // Setup LAN8720A default registers
    mdio.setup_lan8720a(0);
    
    // Simulate link coming up
    mdio.simulate_link_up_100_fd(0);
    
    // Test your PHY driver
    let phy = Lan8720a::new(0);
    assert!(phy.is_link_up(&mut mdio).unwrap());
}
```

**Features:**
- Register map with read/write tracking
- Pre-configured LAN8720A setup
- Link state simulation (`simulate_link_up_100_fd()`, `simulate_link_down()`)

### Mock Delay (`test_utils.rs`)

**Status:** ✅ **IMPLEMENTED**

```rust
use crate::test_utils::MockDelay;

let mut delay = MockDelay::new();
delay.delay_ns(1_000_000);
assert_eq!(delay.total_ms(), 1);
```

### PHY Register Constants (`test_utils.rs`)

Test-friendly constants available:
- `phy_regs`: Register addresses (BMCR, BMSR, PHYIDR1, etc.)
- `bmcr_bits`: BMCR bit definitions
- `bmsr_bits`: BMSR bit definitions
- `anlpar_bits`: ANLPAR bit definitions

---

## Coverage Goals

### Unit Test Coverage

| Module | Target | Current | Status |
|--------|--------|---------|--------|
| `descriptor/` | 90% | 30 tests | ✅ |
| `config.rs` | 85% | 19 tests | ✅ |
| `error.rs` | 80% | 22 tests | ✅ |
| `mac.rs` (InterruptStatus) | 70% | 28 tests | ✅ |
| `hal/mdio.rs` | 80% | 14 tests | ✅ |
| `phy/lan8720a.rs` | 90% | 46 tests | ✅ |
| `dma.rs` | 75% | 2 tests | ✅ |
| `asynch.rs` | 75% | 0 tests | 🔲 Planned |
| `smoltcp.rs` | 60% | 0 tests | 🔲 Planned |

### Integration Test Requirements

| Category | Minimum Tests | Status |
|----------|---------------|--------|
| Initialization | 4 | 🔲 Planned |
| PHY Communication | 5 | 🔲 Planned |
| Loopback | 6 | 🔲 Planned |
| Real Network | 5 | 🔲 Planned |
| Interrupts | 4 | 🔲 Planned |
| Error Handling | 6 | 🔲 Planned |
| Performance | 5 | 🔲 Planned |

---

## Running Tests

### Host Unit Tests

```bash
# Run all unit tests
cargo test

# Run specific module tests
cargo test descriptor
cargo test config
cargo test phy::lan8720a

# Run with verbose output
cargo test -- --nocapture

# List all tests
cargo test -- --list
```

### Hardware Integration Tests

```bash
# Build for ESP32
cargo build --target xtensa-esp32-none-elf --release --example integration_tests

# Flash and run
espflash flash --monitor target/xtensa-esp32-none-elf/release/examples/integration_tests
```

---

## Appendix: Test Constants

### PHY Register Addresses

```rust
const PHY_REG_BMCR: u8 = 0x00;      // Basic Mode Control
const PHY_REG_BMSR: u8 = 0x01;      // Basic Mode Status
const PHY_REG_PHYID1: u8 = 0x02;    // PHY ID 1
const PHY_REG_PHYID2: u8 = 0x03;    // PHY ID 2
const PHY_REG_ANAR: u8 = 0x04;      // Auto-Neg Advertisement
const PHY_REG_ANLPAR: u8 = 0x05;    // Auto-Neg Link Partner
```

### BMCR/BMSR Bits

```rust
// BMCR bits
const BMCR_RESET: u16 = 1 << 15;
const BMCR_LOOPBACK: u16 = 1 << 14;
const BMCR_SPEED_100: u16 = 1 << 13;
const BMCR_AN_ENABLE: u16 = 1 << 12;
const BMCR_DUPLEX_FULL: u16 = 1 << 8;

// BMSR bits
const BMSR_LINK_UP: u16 = 1 << 2;
const BMSR_AN_COMPLETE: u16 = 1 << 5;
```
