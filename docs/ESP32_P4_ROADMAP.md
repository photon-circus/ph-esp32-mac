# ESP32-P4 Support Roadmap

This document defines a concrete, sprint-based plan for bringing ESP32-P4
Ethernet MAC support to `ph-esp32-mac`. Each sprint is scoped to be
independently mergeable and testable.

---

## Table of Contents

- [Background](#background)
- [Current State](#current-state)
- [Key Differences: ESP32 vs ESP32-P4](#key-differences-esp32-vs-esp32-p4)
- [Sprint Overview](#sprint-overview)
- [Sprint 1 — Register and Constant Foundations](#sprint-1--register-and-constant-foundations)
- [Sprint 2 — GPIO Matrix and Pin Mapping](#sprint-2--gpio-matrix-and-pin-mapping)
- [Sprint 3 — Clock, Reset, and HAL Bring-Up](#sprint-3--clock-reset-and-hal-bring-up)
- [Sprint 4 — DMA Descriptor Validation](#sprint-4--dma-descriptor-validation)
- [Sprint 5 — Driver Core Integration](#sprint-5--driver-core-integration)
- [Sprint 6 — CI, Toolchain, and Build Gating](#sprint-6--ci-toolchain-and-build-gating)
- [Sprint 7 — PHY and Board Support](#sprint-7--phy-and-board-support)
- [Sprint 8 — Integration Facades](#sprint-8--integration-facades)
- [Sprint 9 — Hardware Validation and QA](#sprint-9--hardware-validation-and-qa)
- [Sprint 10 — Documentation and Release](#sprint-10--documentation-and-release)
- [Risk Register](#risk-register)
- [Open Questions](#open-questions)
- [Related Documents](#related-documents)

---

## Background

The ESP32-P4 is a RISC-V based SoC (provisionally `riscv32imafc-esp-espidf`;
see [Open Questions](#open-questions) item 6) with an Ethernet MAC peripheral
that shares architectural lineage with the ESP32 EMAC but differs in register
addresses, GPIO routing, DMA cache-line alignment, and clock/reset paths. The `ph-esp32-mac` crate already contains experimental
feature-gated placeholders for ESP32-P4 (`feature = "esp32p4"`), but no
functional implementation exists.

### Goals

- Bring ESP32-P4 EMAC to parity with the ESP32 driver feature set.
- Maintain a single crate with compile-time chip selection via Cargo features.
- Preserve `no_std`, `no_alloc` invariants throughout.
- Keep the ESP32 code path fully stable; no regressions allowed.

### Non-Goals

- WiFi or Bluetooth support.
- Runtime chip detection (features are compile-time only).
- Upstream esp-hal changes (consume only).

---

## Current State

The following table summarises what exists today under `feature = "esp32p4"`:

| Component | Status | Location |
|-----------|--------|----------|
| Feature flag | ✅ Defined | `Cargo.toml` |
| Mutual exclusivity guard | ✅ Enforced | `src/lib.rs` |
| Hidden from docs | ✅ `cfg_attr(docsrs, doc(cfg_hide))` | `src/lib.rs` |
| Register base addresses | ✅ Defined | `src/internal/register/mod.rs` |
| DMA descriptor alignment | ✅ 64-byte | `src/internal/dma/descriptor/{rx,tx}.rs` |
| DMA descriptor size | ✅ 64 bytes | `src/internal/dma/descriptor/{rx,tx}.rs` |
| GPIO pin mapping | ❌ Placeholder (all zeros) | `src/internal/gpio_pins.rs` |
| IO_MUX offsets | ❌ Stub returns 0 | `src/internal/register/gpio.rs` |
| Clock bring-up | ❌ Not implemented | `src/hal/clock.rs` |
| Reset sequence | ❌ Not implemented | `src/hal/reset.rs` |
| MDIO configuration | ❌ Not verified | `src/hal/mdio.rs` |
| Board helpers | ❌ None | `src/boards/` |
| CI build target | ❌ None | `.github/workflows/ci.yml` |

---

## Key Differences: ESP32 vs ESP32-P4

| Aspect | ESP32 | ESP32-P4 |
|--------|-------|----------|
| CPU architecture | Xtensa LX6 | RISC-V (RV32IMAFC) |
| Rust target | `xtensa-esp32-none-elf` | TBD (see [Open Questions](#open-questions) item 6) |
| DMA register base | `0x3FF6_9000` | `0x5008_4000` |
| MAC register base | `0x3FF6_A000` | `0x5008_5000` |
| EXT register base | `0x3FF6_9800` | `0x5008_4800` |
| DMA descriptor alignment | 4 bytes | 64 bytes (cache-line) |
| DMA descriptor size | 32 bytes | 64 bytes |
| RMII pin set | Fixed (GPIO 19, 21, 22, 25, 26, 27) | TBD — verify against TRM |
| IO_MUX base | `0x3FF4_9000` | TBD |
| Clock gate register | `DPORT_WIFI_CLK_EN_REG` | TBD — HP system clock tree |
| GPIO matrix signals | Indices 200/201 | TBD |

---

## Sprint Overview

```text
Sprint  Scope                            Depends On  Est. Effort
──────  ───────────────────────────────   ──────────  ───────────
  1     Register & constant foundations   —           S
  2     GPIO matrix & pin mapping         1           M
  3     Clock, reset, HAL bring-up        1           M
  4     DMA descriptor validation         1           S
  5     Driver core integration           2, 3, 4     L
  6     CI, toolchain, build gating       1           S
  7     PHY & board support               5           M
  8     Integration facades               5           M
  9     Hardware validation & QA          5, 7        L
 10     Documentation & release           All         M

Effort key: S = small (< 1 day), M = medium (1-3 days), L = large (3+ days)
```

Sprints 2, 3, 4, and 6 can proceed in parallel after Sprint 1. Sprint 5 is
the integration point that depends on all foundation work.

---

## Sprint 1 — Register and Constant Foundations

**Goal**: Verify and complete the ESP32-P4 register map so all subsequent
sprints build on a validated base.

### Deliverables

1. Audit `src/internal/register/mod.rs` base addresses against the ESP32-P4
   Technical Reference Manual (TRM).
2. Add any missing P4-specific register offsets or bit-field definitions.
3. Verify or correct `src/internal/constants.rs` for P4-specific values
   (e.g., max frame sizes, FIFO depths, bus widths).
4. Add compile-time assertions for register layout correctness.

### Acceptance Criteria

- `cargo check --no-default-features --features esp32p4` compiles without
  errors for all register modules.
- All register base addresses carry a doc comment citing the TRM section.

### Files Touched

- `src/internal/register/mod.rs`
- `src/internal/constants.rs`

---

## Sprint 2 — GPIO Matrix and Pin Mapping

**Goal**: Implement the full GPIO routing table for ESP32-P4 EMAC signals.

### Deliverables

1. Define RMII pin assignments in `src/internal/gpio_pins.rs::esp32p4`.
2. Implement IO_MUX offset table in `src/internal/register/gpio.rs` for the
   ESP32-P4 GPIO address space.
3. Map EMAC signal indices (TX_EN, TXD0, TXD1, RXD0, RXD1, CRS_DV, MDC,
   MDIO, CLK) to their P4 GPIO matrix function numbers.
4. Add unit tests verifying pin constants are non-zero and within valid GPIO
   range.

### Acceptance Criteria

- GPIO module compiles and passes tests under `--features esp32p4`.
- Each pin constant has a doc comment referencing the TRM table.

### Files Touched

- `src/internal/gpio_pins.rs`
- `src/internal/register/gpio.rs`

---

## Sprint 3 — Clock, Reset, and HAL Bring-Up

**Goal**: Implement the P4-specific clock enable and reset sequences.

### Deliverables

1. Identify the ESP32-P4 clock gate register and bit for the EMAC peripheral.
2. Add `#[cfg(feature = "esp32p4")]` branches in `src/hal/clock.rs` for
   clock enable/disable.
3. Verify or add P4-specific reset assertions in `src/hal/reset.rs`.
4. Confirm MDIO clock divider calculation in `src/hal/mdio.rs` works with
   the P4 system clock frequency.

### Acceptance Criteria

- `cargo check --no-default-features --features esp32p4` compiles for all
  HAL modules.
- Clock and reset functions have doc comments documenting the P4 register
  addresses and bit positions used.

### Files Touched

- `src/hal/clock.rs`
- `src/hal/reset.rs`
- `src/hal/mdio.rs`

---

## Sprint 4 — DMA Descriptor Validation

**Goal**: Confirm the DMA descriptor layout and alignment are correct for the
ESP32-P4 cache architecture.

### Deliverables

1. Verify the 64-byte alignment and 64-byte descriptor size against the
   ESP32-P4 TRM DMA chapter.
2. Audit the extended descriptor fields (words 4-7 in the 64-byte layout)
   for any P4-specific control bits.
3. Add or update compile-time size and alignment assertions under
   `#[cfg(feature = "esp32p4")]`.
4. Verify that `RxDescriptorRing` and `TxDescriptorRing` initialisation
   logic produces correct linked lists with 64-byte stride.

### Acceptance Criteria

- All existing DMA descriptor tests pass under `--features esp32p4`.
- New assertions verify `size_of::<RxDescriptor>() == 64` and
  `align_of::<RxDescriptor>() == 64` when the P4 feature is active.

### Files Touched

- `src/internal/dma/descriptor/rx.rs`
- `src/internal/dma/descriptor/tx.rs`

---

## Sprint 5 — Driver Core Integration

**Goal**: Wire up the foundation work so the `Emac` driver compiles and
initialises correctly for ESP32-P4.

### Deliverables

1. Ensure `Emac::new()`, `init()`, `start()`, and `stop()` compile and
   follow the correct P4 HAL paths.
2. Trace the full init sequence (clock → reset → GPIO → DMA → MAC config)
   and verify each step dispatches to P4 code where needed.
3. Add `#[cfg]` branches for any driver-level differences (e.g., FIFO
   thresholds, bus mode register defaults).
4. Write host-side unit tests exercising the init sequence under
   `--features esp32p4` (mock register writes where needed).

### Acceptance Criteria

- `cargo test --no-default-features --features esp32p4` passes all unit
  tests.
- `cargo check --no-default-features --features esp32p4` compiles the full
  crate without warnings.

### Files Touched

- `src/driver/`
- `src/lib.rs`

---

## Sprint 6 — CI, Toolchain, and Build Gating

**Goal**: Add CI coverage for the ESP32-P4 feature so regressions are caught
automatically.

### Deliverables

1. Add a CI job that runs `cargo check --no-default-features --features
   esp32p4` using the RISC-V target (`riscv32imafc-esp-espidf` or the
   appropriate `riscv32` bare-metal target).
2. Add a clippy job for `--features esp32p4`.
3. Add a doc-build job for `--features esp32p4`.
4. Ensure the `docsrs` job includes P4 feature coverage.

### Acceptance Criteria

- CI runs a P4 build check on every push and PR.
- Clippy reports zero warnings under `--features esp32p4`.

### Files Touched

- `.github/workflows/ci.yml`

---

## Sprint 7 — PHY and Board Support

**Goal**: Add at least one reference PHY driver and board helper for a
commonly available ESP32-P4 development board.

### Deliverables

1. Identify the reference ESP32-P4 Ethernet development board and its PHY
   (e.g., the ESP32-P4 Function EV Board with IP101GRI or similar).
2. Verify or adapt the `PhyDriver` trait implementation for the board's PHY.
3. Add a board helper module under `src/boards/` (gated on
   `feature = "esp32p4"`).
4. Document the board's pin configuration, clock source, and PHY address.

### Acceptance Criteria

- Board helper compiles under `--features esp32p4,esp-hal`.
- Board module includes a complete doc comment with wiring table.

### Files Touched

- `src/boards/` (new module)
- `src/phy/` (if a new PHY driver is needed)

---

## Sprint 8 — Integration Facades

**Goal**: Ensure smoltcp, embassy-net, and esp-hal facades work with ESP32-P4.

### Deliverables

1. Audit `src/integration/esp_hal.rs` for any ESP32-specific assumptions
   (e.g., peripheral type paths, clock types).
2. Add `#[cfg]` branches or generics where the esp-hal peripheral types
   differ between chips.
3. Verify `src/integration/smoltcp.rs` and `src/integration/embassy_net.rs`
   are chip-agnostic (they should be, since they operate on the driver
   abstraction).
4. Build the full feature matrix (`esp32p4 + smoltcp`, `esp32p4 + async`,
   `esp32p4 + embassy-net`) and fix any compilation errors.

### Acceptance Criteria

- All feature combinations compile: `esp32p4`, `esp32p4,smoltcp`,
  `esp32p4,async`, `esp32p4,embassy-net`, `esp32p4,esp-hal`.
- No chip-specific types leak into the integration layer APIs.

### Files Touched

- `src/integration/esp_hal.rs`
- `src/integration/smoltcp.rs` (if needed)
- `src/integration/embassy_net.rs` (if needed)

---

## Sprint 9 — Hardware Validation and QA

**Goal**: Validate the driver on real ESP32-P4 hardware.

### Deliverables

1. Port `apps/qa-runner/` to support the ESP32-P4 board (RISC-V target,
   P4 board config).
2. Port or create at least one `apps/examples/` app demonstrating P4 EMAC
   usage (e.g., DHCP + ping).
3. Run the full QA test suite on hardware:
   - Link up / link down detection
   - TX/RX at 100 Mbps
   - MDIO PHY register read/write
   - Interrupt-driven receive
   - Flow control (if supported)
4. Document hardware test results and any errata encountered.

### Acceptance Criteria

- QA runner passes all tests on ESP32-P4 hardware.
- At least one example app demonstrates successful network communication.

### Files Touched

- `apps/qa-runner/`
- `apps/examples/`
- `xtask/` (build target additions)

---

## Sprint 10 — Documentation and Release

**Goal**: Update all documentation, remove the "experimental" label, and
prepare a release.

### Deliverables

1. Update `README.md` to list ESP32-P4 as supported.
2. Update `docs/ARCHITECTURE.md` scope section.
3. Update `docs/DESIGN.md` scope and non-goals sections.
4. Remove `doc(cfg_hide)` for `esp32p4` in `src/lib.rs`.
5. Add a `CHANGELOG.md` entry for P4 support.
6. Update `AGENTS.md` and `CLAUDE.md` to reflect P4 as supported.
7. Update `RELEASE.md` pre-flight checklist to include P4 verification.
8. Publish a new minor version (e.g., `0.2.0`).

### Acceptance Criteria

- No documentation references "placeholder" or "experimental" for ESP32-P4.
- `cargo doc --features esp32p4` builds clean documentation showing P4
  support.

### Files Touched

- `README.md`
- `CHANGELOG.md`
- `RELEASE.md`
- `AGENTS.md`, `CLAUDE.md`
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `src/lib.rs`

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| TRM register map has undocumented errata | Medium | High | Validate every register write against silicon; keep ESP32 code path unchanged |
| P4 EMAC peripheral differs more than expected | Medium | High | Feature-gate at the HAL layer; avoid chip-specific logic in the driver core |
| No P4 hardware available for testing | Low | Critical | Block Sprint 9 until hardware is sourced; use CI build checks as a gate |
| esp-hal P4 support is incomplete | Medium | Medium | Depend only on stable esp-hal APIs; abstract chip-specific types behind the facade |
| Cache coherency issues with 64-byte DMA descriptors | Medium | High | Use volatile access patterns; verify with cache-disabled test first |
| RISC-V toolchain instability | Low | Medium | Pin toolchain version in `rust-toolchain.toml` and CI |

---

## Open Questions

These must be resolved before or during the corresponding sprint:

1. **What is the canonical ESP32-P4 Ethernet dev board?** Required for
   Sprint 7 (board support) and Sprint 9 (hardware QA).
2. **What PHY is used on the reference board?** Required for Sprint 7.
   If not LAN8720A, a new PHY driver may be needed.
3. **What are the exact RMII GPIO assignments on ESP32-P4?** Required for
   Sprint 2. Must be confirmed against the TRM.
4. **What is the P4 system clock frequency for MDIO divider calculation?**
   Required for Sprint 3.
5. **Does the P4 EMAC support enhanced descriptors (EDMA)?** If so,
   Sprint 4 may need additional descriptor field support.
6. **What Rust target triple should be used for bare-metal P4?** The
   `riscv32imafc-esp-espidf` target includes the ESP-IDF sysroot;
   a `riscv32imafc-unknown-none-elf` target may be more appropriate for
   `no_std` builds. Required for Sprint 6.

---

## Related Documents

- [ARCHITECTURE.md](ARCHITECTURE.md) — System layout and data flow.
- [DESIGN.md](DESIGN.md) — Design decisions, constraints, and invariants.
- [TESTING.md](TESTING.md) — Test strategy and coverage.
- [DOCUMENTATION_STANDARDS.md](DOCUMENTATION_STANDARDS.md) — Writing standards.
