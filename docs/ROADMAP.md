# Roadmap

This document tracks planned feature improvements, testing infrastructure
enhancements, and known areas for future work. Items are grouped by priority
and domain.

---

## Table of Contents

- [Near-Term (next minor release)](#near-term-next-minor-release)
- [Medium-Term (0.2.x cycle)](#medium-term-02x-cycle)
- [Long-Term / Exploratory](#long-term--exploratory)
- [Integration Testing Infrastructure](#integration-testing-infrastructure)
- [Code Quality Improvements](#code-quality-improvements)
- [Documentation](#documentation)
- [Non-Goals](#non-goals)

---

## Near-Term (next minor release)

### Expanded Host Test Coverage

- Add host-level unit tests for `EmacPhyBundle` link polling and timeout
  logic (testable with `MockMdioBus` and `MockDelay`).
- Add unit tests for `EmbassyEmacState` waker registration and
  `on_interrupt` dispatch paths.
- Add unit tests for `filtering.rs` error paths (duplicate filter, filter
  removal when not present).

### Error Variant Hygiene

- ~~`remove_mac_filter` returned `DmaError::InvalidLength` for a config-level
  "not found" condition. This has been corrected to
  `ConfigError::InvalidConfig`.~~ *(done)*

### Feature-Flag Test Matrix

- Extend CI to run `cargo test --lib` under each individual feature flag
  (`smoltcp`, `async`, `embassy-net`, `critical-section`) in addition to the
  current combined-feature run. This catches conditional compilation issues
  that hide behind the "all features" build.

---

## Medium-Term (0.2.x cycle)

### Hardware-in-the-Loop (HIL) CI

- Integrate a self-hosted GitHub Actions runner connected to a WT32-ETH01
  board.
- Run the `qa-runner` test suite automatically on every PR that touches
  `src/` or `apps/qa-runner/`.
- Report pass/fail results as a CI check with log artifacts.

### smoltcp / Embassy Simulated Integration Tests

- Create a host-side integration test that exercises the full
  `Device::receive` → `Device::transmit` path using a mock `Emac` stub.
- Verify that `EmbassyEmac` correctly wakes RX/TX tasks and transitions
  link state through `EmbassyEmacState`.

### Additional PHY Drivers

- `RTL8201`: Common alternative RMII PHY.
- Generic MII-mode PHY for boards that use the MII interface.
- Each new driver should include host-testable register logic using the
  existing `MockMdioBus` pattern.

### Runtime Statistics

- Expose TX/RX frame counters and error counters through a `Statistics`
  struct.
- Keep counters in the `Emac` struct (no allocation) and optionally
  expose them via defmt/log.

### Power Management

- Add `Emac::suspend()` / `Emac::resume()` for light-sleep support.
- Gate EMAC clocks during suspend; restore DMA state on resume.
- Expose PHY power-down/wake through `PhyDriver`.

---

## Long-Term / Exploratory

### ESP32-P4 Support

- The `esp32p4` feature flag is currently a placeholder.
- Bring-up requires new register definitions, a different DMA engine, and
  hardware validation. Track this separately once ESP32-P4 Ethernet
  silicon is available.

### Jumbo Frame Support

- Currently `BUF_SIZE` caps at the compile-time const generic.
- Investigate whether the ESP32 EMAC DMA supports frames larger than the
  standard 1522-byte maximum.

### Hardware Checksum Offload

- The EMAC supports TX/RX IP/TCP/UDP checksum offload.
- Currently software checksums are used for maximum compatibility.
- Expose hardware checksum control through `EmacConfig` and report
  offload capabilities to smoltcp/embassy-net.

### Scatter-Gather TX

- Allow transmitting frames that span multiple DMA descriptors without
  copying into a contiguous buffer first.
- This would improve TX throughput for large frames on memory-constrained
  systems.

---

## Integration Testing Infrastructure

This section describes concrete improvements to the testing pipeline.

### 1. Host Mock Enhancements

| Enhancement | Purpose |
|-------------|---------|
| `MockEmac` struct | Testable `Emac` stub for integration-layer tests without hardware |
| `MockDmaEngine` | Simulates DMA frame delivery and TX completion for end-to-end flow |
| Error injection in `MockMdioBus` | Simulate MDIO timeouts and bus errors for resilience testing |
| Configurable `MockDelay` | Simulate wall-clock time to test timeout and polling logic |

### 2. CI Pipeline Improvements

| Improvement | Details |
|-------------|---------|
| Per-feature test jobs | Run `cargo test --lib --features X` for each optional feature individually |
| Nightly toolchain test | Catch upcoming breakage early with a nightly `cargo test` job |
| Dependency audit | Add `cargo audit` or `cargo deny` job to flag vulnerable dependencies |
| Binary size tracking | Record `.text` / `.data` / `.bss` sizes for the embedded target and alert on regressions |

### 3. Hardware-in-the-Loop (HIL) Pipeline

```text
┌────────────┐     ┌────────────┐     ┌────────────┐
│  GitHub PR  │────▶│  Self-hosted│────▶│  WT32-ETH01│
│  (ci.yml)   │     │  Runner     │     │  Board     │
└────────────┘     └────────────┘     └────────────┘
                         │
                         ▼
                   ┌────────────┐
                   │ qa-runner   │
                   │ test output │
                   └────────────┘
```

- Runner flashes the `qa-runner` binary via `cargo xtask run qa-runner`.
- Serial output is captured and parsed for pass/fail verdicts.
- Results are uploaded as GitHub Actions artifacts.
- Failures block PR merge.

### 4. Coverage Improvements

- Add feature-gated coverage runs (`smoltcp`, `async`, `embassy-net`) to
  the coverage job so conditional code paths are measured.
- Track coverage trends over time and set a minimum threshold (target: 80%
  line coverage for host-testable code).

---

## Code Quality Improvements

- Ensure every `unsafe` block has a `// SAFETY:` comment per project
  standards.
- Replace magic bit-position literals in interrupt enable logic with named
  constants from the register module.
- Add `#[must_use]` to methods that return important results (e.g.,
  `handle_interrupt`).
- Audit all `#[allow(dead_code)]` annotations in `internal/` and remove
  those guarding genuinely unreachable code.

---

## Documentation

- Add a `docs/README.md` index linking all documentation files.
- Expand `docs/TESTING.md` with the integration testing infrastructure
  plan (covered above).
- Add architecture decision records (ADRs) for key design choices
  (e.g., why raw pointers in smoltcp/embassy-net tokens).

---

## Non-Goals

These items are explicitly out of scope for this crate:

- WiFi support
- Dynamic memory allocation or runtime buffer resizing
- Multi-chip SPI Ethernet controllers (W5500, ENC28J60)
- Stable ESP32-P4 support in the 0.1.x or 0.2.x series
