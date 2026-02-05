# Testing Documentation

This document describes the testing strategy, current coverage, and known limitations for the ESP32 EMAC driver.

---

## Table of Contents

1. [Testing Philosophy](#testing-philosophy)
2. [Test Summary](#test-summary)
3. [Unit Tests](#unit-tests-host)
4. [Integration Tests](#integration-tests-hardware)
5. [Coverage Analysis](#coverage-analysis)
6. [Known Limitations](#known-limitations)
7. [Running Tests](#running-tests)

---

## Testing Philosophy

### Two-Tier Strategy

This project uses a two-tier testing approach:

| Tier | Environment | Purpose |
|------|-------------|---------|
| **Unit Tests** | Host (x86/ARM) | Test logic, algorithms, state machines without hardware |
| **Integration Tests** | ESP32 hardware | Test hardware interaction, DMA, PHY communication |

### Design Principles

1. **Test what can be tested on host** — Business logic, parsing, state machines, and configuration should have exhaustive unit tests. These run fast and catch bugs early.

2. **Mock hardware interfaces** — Use traits (`MdioBus`, `DelayNs`) and mock implementations for testing PHY drivers and timing-sensitive code without hardware.

3. **Hardware tests verify integration** — Integration tests confirm that register writes, DMA operations, and PHY communication work on real hardware. They don't need to test every edge case—that's what unit tests are for.

4. **Accept hardware-only gaps** — Some code (register accessors, clock configuration) can only run on ESP32. Zero coverage for these modules is expected and acceptable.

### Test Coverage Philosophy

We prioritize coverage based on risk and testability:

| Priority | Code Type | Target Coverage | Rationale |
|----------|-----------|-----------------|-----------|
| **High** | Configuration, errors, parsing | 90%+ | Pure logic, easy to test, high bug impact |
| **Medium** | PHY drivers, DMA logic | 70%+ | Uses mocks, moderate complexity |
| **Low** | Hardware register access | 0% | Requires real hardware, tested via integration |

---

## Test Summary

| Category | Count | Status |
|----------|-------|--------|
| Unit Tests (host) | 299 | ✅ All passing |
| Integration Tests (hardware) | 47 | ✅ All passing |
| **Total** | **346** | ✅ |

### Code Coverage (llvm-cov)

| Metric | Coverage |
|--------|----------|
| Lines | 63.33% |
| Regions | 67.41% |
| Functions | 64.24% |

**Note:** Coverage includes 0% for hardware-only modules. Excluding those, testable code coverage is approximately 78%.

---

## Unit Tests (Host)

Unit tests run on the development machine with `cargo test --lib`. They test logic that doesn't require ESP32 hardware.

### Module Coverage

| Module | Tests | Line Coverage | Notes |
|--------|-------|---------------|-------|
| `constants.rs` | 29 | **100%** | Frame sizes, timing, defaults |
| `error.rs` | 22 | **98%** | Error types and conversions |
| `sync_primitives.rs` | 14 | **96%** | AtomicWaker, CriticalSectionCell |
| `config.rs` | 19 | **93%** | Builder pattern, enums |
| `phy/generic.rs` | — | **89%** | Generic PHY traits |
| `descriptor/tx.rs` | 17 | **85%** | TX descriptor layout and control |
| `test_utils.rs` | 5 | **82%** | Mock implementations |
| `phy/lan8720a.rs` | 46 | **81%** | LAN8720A PHY driver |
| `sync.rs` | 21 | **80%** | SharedEmac wrappers |
| `hal/mdio.rs` | 32 | **74%** | MDIO clock, status parsing |
| `dma.rs` | 47 | **69%** | DMA engine, descriptor rings |
| `descriptor/rx.rs` | 13 | **66%** | RX descriptor status parsing |
| `asynch.rs` | 4 | **48%** | Async wakers (limited without runtime) |
| `mac.rs` | 28 | **35%** | Hardware-heavy, InterruptStatus tested |
| `smoltcp.rs` | 9 | — | Trait implementations |

### Key Test Categories

**Descriptor Tests (30 tests)**
- Layout verification (size, alignment)
- Ownership bit manipulation
- Status flag parsing (first/last/error)
- TX control bits and checksum modes

**PHY Driver Tests (46 tests)**
- PHY ID verification
- Soft reset sequence
- Link status detection
- Auto-negotiation flow
- Forced speed/duplex
- LAN8720A vendor features

**DMA Tests (47 tests)**
- Ring navigation and wraparound
- Ownership tracking
- TX/RX flow simulation with `MockDescriptor`
- Buffer management

**Configuration Tests (19 tests)**
- Builder pattern chaining
- Enum conversions
- MAC address filtering options

---

## Integration Tests (Hardware)

Integration tests run on ESP32 hardware (WT32-ETH01 board) and verify real hardware operation.

### Hardware Requirements

- WT32-ETH01 board (ESP32 + LAN8720A PHY)
- Ethernet cable connected to a switch/router
- USB-TTL adapter for flashing

### Test Organization

Tests are organized into 9 groups with unique IDs (`IT-{GROUP}-{NUMBER}`):

```
integration_tests/
├── wt32_eth01.rs           # Entry point
└── tests/
    ├── mod.rs              # Module exports
    ├── framework.rs        # TestResult, TestStats, EMAC static
    ├── group1_register.rs  # IT-1-xxx: Register access
    ├── group2_init.rs      # IT-2-xxx: Initialization
    ├── group3_phy.rs       # IT-3-xxx: PHY communication
    ├── group4_emac.rs      # IT-4-xxx: EMAC operations
    ├── group5_link.rs      # IT-5-xxx: Link status
    ├── group6_smoltcp.rs   # IT-6-xxx: smoltcp integration
    ├── group7_state.rs     # IT-7-xxx: State & interrupts
    ├── group8_advanced.rs  # IT-8-xxx: Advanced features
    └── group9_edge.rs      # IT-9-xxx: Edge cases
```

### Test ID Reference

#### Group 1: Register Access (4 tests)
| ID | Test | Verifies |
|----|------|----------|
| IT-1-001 | Clock enable | DPORT register write enables EMAC clock |
| IT-1-002 | DMA registers | DMA registers readable after clock enable |
| IT-1-003 | MAC registers | MAC configuration registers accessible |
| IT-1-004 | Extension registers | ESP32-specific extension registers accessible |

#### Group 2: EMAC Initialization (3 tests)
| ID | Test | Verifies |
|----|------|----------|
| IT-2-001 | EMAC init | Full initialization with config completes |
| IT-2-002 | RMII pins | GPIO MUX configured for RMII |
| IT-2-003 | DMA descriptors | Descriptor ring properly linked |

#### Group 3: PHY Communication (3 tests)
| ID | Test | Verifies |
|----|------|----------|
| IT-3-001 | PHY ID read | MDIO reads LAN8720A OUI (0x0007C0) |
| IT-3-002 | PHY init | Soft reset and auto-negotiation start |
| IT-3-003 | Link up | Link comes up within 5 second timeout |

#### Group 4: EMAC Operations (4 tests)
| ID | Test | Verifies |
|----|------|----------|
| IT-4-001 | Start | `start()` enables TX/RX DMA |
| IT-4-002 | Transmit | Broadcast frame transmitted without error |
| IT-4-003 | Receive | Packets received when traffic present |
| IT-4-004 | Stop/start | Stop and restart cycle completes |

#### Group 5: Link Status (1 test)
| ID | Test | Verifies |
|----|------|----------|
| IT-5-001 | Link query | `is_link_up()` returns correct state |

#### Group 6: smoltcp Integration (3 tests)
| ID | Test | Verifies |
|----|------|----------|
| IT-6-001 | Interface create | smoltcp `Interface` construction |
| IT-6-002 | Capabilities | `Device` trait returns correct MTU |
| IT-6-003 | Poll | `Interface::poll()` runs without panic |

#### Group 7: State & Interrupts (11 tests)
| ID | Test | Verifies |
|----|------|----------|
| IT-7-001 | State running | `state()` returns `Running` after start |
| IT-7-002 | State stopped | `state()` returns `Stopped` after stop |
| IT-7-003 | TX ready | `tx_ready()` and descriptor count match |
| IT-7-004 | Can transmit | `can_transmit()` for 64/512/1518/2000 bytes |
| IT-7-005 | Backpressure | Fills TX buffer, detects not ready |
| IT-7-006 | Peek RX | `peek_rx_length()` consistent with `rx_available()` |
| IT-7-007 | RX frames | `rx_frames_waiting()` returns correct count |
| IT-7-008 | Interrupt status | `interrupt_status()` reads all flags |
| IT-7-009 | Clear interrupts | `clear_all_interrupts()` clears pending |
| IT-7-010 | Handle interrupt | `handle_interrupt()` atomic read+clear |
| IT-7-011 | Frame sizes | TX min (64) through max (1518) frames |

#### Group 8: Advanced Features (7 tests)
| ID | Test | Verifies |
|----|------|----------|
| IT-8-001 | Promiscuous | Enable/disable promiscuous mode |
| IT-8-002 | Promiscuous RX | Receives traffic in promiscuous mode |
| IT-8-003 | PHY capabilities | Reads supported speed/duplex modes |
| IT-8-004 | Force link | Manually sets 10M/100M speed |
| IT-8-005 | TX interrupt | `enable_tx_interrupt()` toggles |
| IT-8-006 | RX interrupt | `enable_rx_interrupt()` toggles |
| IT-8-007 | TX IRQ fires | Interrupt status set after transmit |

#### Group 9: Edge Cases (11 tests)
| ID | Test | Verifies |
|----|------|----------|
| IT-9-001 | MAC filter | Add/remove unicast filter |
| IT-9-002 | Multi-filter | Multiple filter slots, clear all |
| IT-9-003 | Hash filter | Hash table for multicast |
| IT-9-004 | Pass multicast | Toggle pass-all-multicast |
| IT-9-005 | VLAN filter | Set VID, disable filter |
| IT-9-006 | Flow config | Read flow control config |
| IT-9-007 | Flow check | `check_flow_control()` runs |
| IT-9-008 | Energy detect | PHY EDPD enable/disable |
| IT-9-009 | RX IRQ fires | Interrupt status set on receive |
| IT-9-010 | Async wakers | API availability check |
| IT-9-011 | Restore state | Clean up for monitoring mode |

---

## Coverage Analysis

### What Is Tested Well

| Feature | Unit | Integration | Confidence |
|---------|------|-------------|------------|
| Configuration builder | ✅ 93% | ✅ IT-2-001 | **High** |
| Error types | ✅ 98% | — | **High** |
| TX/RX descriptors | ✅ 85%/66% | ✅ IT-7-xxx | **High** |
| LAN8720A PHY driver | ✅ 81% | ✅ IT-3-xxx | **High** |
| Interrupt status parsing | ✅ 100% | ✅ IT-7-008 | **High** |
| smoltcp Device trait | ✅ | ✅ IT-6-xxx | **High** |
| Start/stop/transmit/receive | — | ✅ IT-4-xxx | **High** |

### What Is Partially Tested

| Feature | Status | Gap | Risk |
|---------|--------|-----|------|
| **Async wakers** | API exists test only (IT-9-010) | Full async requires embassy runtime | **Medium** — users need async, but wakers are simple |
| **Flow control** | Config read (IT-9-006/007) | `enable_flow_control()` not tested | **Low** — rarely used in embedded |
| **PHY interrupts** | Not tested | `enable_link_interrupt()` untested | **Low** — polling is standard approach |
| **Checksum offload** | smoltcp config tested | Hardware checksum not verified | **Medium** — smoltcp handles this |

### What Is Not Tested

| Feature | Reason | Justification |
|---------|--------|---------------|
| Register accessors (`register/*.rs`) | Hardware-only | Register layout verified by integration tests working |
| Clock/reset (`hal/clock.rs`, `hal/reset.rs`) | Hardware-only | Would fail immediately if wrong |
| Multi-descriptor frames | Complex setup | Unit tests cover descriptor linking logic |
| CRC error injection | Requires hardware | Would need faulty cable or loopback |
| Full duplex pause frames | Requires 2 boards | Flow control config tested instead |

---

## Known Limitations

### Tests That Don't Verify Behavior

Some integration tests verify that API calls succeed but cannot confirm the hardware actually does what we asked:

#### IT-9-005: VLAN Filter Test

**What it tests:**
```rust
emac.set_vlan_filter(100);  // Returns Ok
emac.disable_vlan_filter(); // Returns Ok
```

**What it doesn't test:**
- That frames without VLAN tag 100 are actually dropped
- That frames with VLAN tag 100 are actually passed

**Why:**
To verify VLAN filtering works, we would need:
1. A traffic generator sending VLAN-tagged frames
2. A way to confirm which frames were received/dropped

**Risk:** **Low** — The register writes are simple. If `set_vlan_filter(100)` writes to the correct register (verified by reading it back), the hardware will filter correctly. The EMAC IP core is well-documented.

**Mitigation:** `is_vlan_filter_enabled()` and `vlan_filter_id()` let us read back the configuration.

#### IT-9-003: Hash Filter Test

**What it tests:**
```rust
emac.set_hash_table(0xFFFF_FFFF_FFFF_FFFF);  // Set all bits
emac.set_hash_table(0);                       // Clear
```

**What it doesn't test:**
- That multicast frames matching hash bits are received
- That non-matching frames are dropped

**Risk:** **Low** — Same rationale as VLAN. Register writes are straightforward.

#### IT-8-002: Promiscuous Mode Test

**What it tests:**
- Enables promiscuous mode
- Listens for 2 seconds
- Reports whether any frames were received

**What it doesn't test:**
- That frames destined for OTHER MAC addresses are received
- Comparison with non-promiscuous reception

**Risk:** **Low** — Promiscuous mode is a single bit in the MAC filter register. The test confirms reception works; comparing promiscuous vs filtered reception would require controlled traffic.

### Tests Dependent on Network Traffic

Several tests produce "soft pass" results when no traffic is present:

| Test | Behavior Without Traffic |
|------|--------------------------|
| IT-4-003 (Packet RX) | Passes with "0 packets received" warning |
| IT-8-002 (Promiscuous RX) | Passes with "no traffic" warning |
| IT-9-009 (RX IRQ fires) | Passes with "no interrupt" warning |

**Justification:** We cannot force external traffic during automated tests. These tests verify the driver doesn't crash and can receive when traffic exists. Manual testing with a traffic generator confirms full functionality.

### Async Testing Limitations

IT-9-010 only verifies the async API exists:
```rust
info!("  Async per-instance waker state available (AsyncEmacState)");
info!("  Full async test requires async feature + runtime - skipping");
```

**What's needed for full async tests:**
- Embassy runtime setup
- `#[embassy_executor::main]` entry point  
- Async task spawning

**Risk:** **Medium** — Async is important for production use. However:
- Waker logic is simple (AtomicWaker is well-tested)
- Interrupt handler is tested (IT-7-010, IT-8-007, IT-9-009)
- Users running embassy will test this in their applications

**Mitigation:** Unit tests for `AtomicWaker` cover the core waker mechanics.

---

## Risk Assessment

### Accepted Risks

| Risk | Severity | Likelihood | Mitigation |
|------|----------|------------|------------|
| VLAN filter misconfigured | Low | Low | Read-back verification exists |
| Hash table misconfigured | Low | Low | Conservative defaults (pass-all) |
| Async race conditions | Medium | Low | AtomicWaker is proven pattern |
| DMA overflow under load | Medium | Low | Tested with backpressure (IT-7-005) |
| PHY interrupt missed | Low | Medium | Polling is primary approach |

### Coverage Trade-offs

| Trade-off | Rationale |
|-----------|-----------|
| 0% coverage on `register/*.rs` | Would require ESP32 emulator or hardware-in-loop CI. Cost exceeds benefit for simple register accessors. |
| 35% coverage on `mac.rs` | Heavy hardware interaction. InterruptStatus (100% covered) is the main testable component. |
| No multi-board flow control test | Would require 2 ESP32 boards and coordinated test. Manual testing sufficient. |
| No CRC error tests | Requires error injection hardware. Trust EMAC IP core validation. |

---

## Running Tests

### Unit Tests (Host)

```bash
# Run all unit tests
cargo test --lib

# Run specific module
cargo test --lib phy::lan8720a

# Verbose output
cargo test --lib -- --nocapture

# Coverage report
cargo llvm-cov --lib --html
```

### Integration Tests (Hardware)

```bash
# From project root (cargo alias)
cargo int

# Build only (no flash)
cargo int-build

# Manually
cd integration_tests
cargo run --release
```

### Interpreting Results

**Unit tests:** All 299 tests should pass. Any failure indicates a bug.

**Integration tests:** 
- Tests should show `[PASS]` for each test ID
- "Warning" messages about no traffic are normal
- Tests are ordered by dependency—if Group 1 fails, later groups will fail

**Coverage report:**
- Focus on modules with `>60%` target
- 0% for `register/*.rs` is expected
- `mac.rs` at 35% is acceptable (hardware-heavy)

---

## Test Infrastructure

### Mock Implementations

| Mock | Purpose | Location |
|------|---------|----------|
| `MockMdioBus` | PHY register simulation | `test_utils.rs` |
| `MockDelay` | Timing verification | `test_utils.rs` |
| `MockDescriptor` | DMA flow testing | `test_utils.rs` |

### MockMdioBus Features

```rust
let mut mdio = MockMdioBus::new();

// Setup LAN8720A defaults
mdio.setup_lan8720a(0);

// Simulate link events
mdio.simulate_link_up_100_fd(0);
mdio.simulate_link_down(0);

// Read/write tracking
let writes = mdio.write_history(0);
```

### MockDescriptor Features

```rust
let mut desc = MockDescriptor::new();

// Ownership tracking
desc.set_owned();
assert!(desc.is_owned());

// Simulate DMA receiving a frame
desc.simulate_receive(1500);
assert!(!desc.is_owned());
assert_eq!(desc.frame_length(), 1500);

// Simulate errors
desc.simulate_error();
assert!(desc.has_error());
```

---

## Appendix: Test Count by Module

| Module | Unit Tests |
|--------|------------|
| `dma.rs` | 47 |
| `phy/lan8720a.rs` | 46 |
| `hal/mdio.rs` | 32 |
| `constants.rs` | 29 |
| `mac.rs` | 28 |
| `error.rs` | 22 |
| `sync.rs` | 21 |
| `config.rs` | 19 |
| `descriptor/tx.rs` | 17 |
| `sync_primitives.rs` | 14 |
| `descriptor/rx.rs` | 13 |
| `smoltcp.rs` | 9 |
| `test_utils.rs` | 5 |
| `asynch.rs` | 4 |
| `descriptor/mod.rs` | 1 |
| **Total** | **299** |
