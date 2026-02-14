# Testing

This document summarizes how to validate the driver and where the testing gaps
remain. Hardware correctness is the highest-risk area and requires on-device
validation.

---

## Table of Contents

- [Host Checks](#host-checks)
- [Hardware Validation](#hardware-validation)
- [CI Coverage](#ci-coverage)
- [Coverage and Gaps](#coverage-and-gaps)
- [Integration Testing Infrastructure](#integration-testing-infrastructure)
- [Notes for Production Use](#notes-for-production-use)

---

## Host Checks

```bash
cargo test --lib
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps
```

Optional coverage (requires `cargo-llvm-cov`):

```bash
cargo llvm-cov --lib
```

---

## Hardware Validation

Hardware QA lives in `apps/qa-runner/` and is the primary way to exercise DMA,
register access, and PHY behavior.

From the repo root:

```bash
cargo xtask build qa-runner
cargo xtask run qa-runner
```

See [apps/qa-runner/README.md](../apps/qa-runner/README.md) for wiring and board
notes.

---

## CI Coverage

CI validates the host toolchain and documentation, and includes an ESP32 target
check. Current CI jobs cover:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --lib`
- `cargo doc --no-deps`
- `cargo +esp check --target xtensa-esp32-none-elf -Zbuild-std=core`

Clippy also runs a feature matrix (default + `smoltcp` + `async` +
`critical-section` + `embassy-net`) to catch feature-gated issues.

---

## Coverage and Gaps

Covered well:
- Configuration, error handling, and parsing logic (host unit tests)
- PHY logic via MDIO mocks (host tests)
- Core DMA ring behavior with mocked descriptors (host tests)

Known gaps and risks:
- Hardware-only paths: register access, clock/reset sequencing, DMA ownership
- Network variability: DHCP and link bring-up depend on external traffic
- Async runtime behavior: waker logic is unit-tested, full behavior is on-target
- Integration layer end-to-end: smoltcp/embassy-net `Device`/`Driver` token
  flow is partially tested (constants and capabilities only)
- `EmacPhyBundle` link polling and timeout logic (testable on host with mocks)
- `EmbassyEmacState` waker dispatch under interrupt status combinations

---

## Integration Testing Infrastructure

This section describes the current integration testing strategy and planned
improvements. See [ROADMAP.md](ROADMAP.md) for the full feature roadmap.

### Current Architecture

```text
┌──────────────────────────────────────────┐
│               Host (CI)                  │
│                                          │
│  cargo test --lib                        │
│  ├── driver/config tests                 │
│  ├── driver/error tests                  │
│  ├── driver/interrupt tests              │
│  ├── phy/lan8720a tests (MockMdioBus)    │
│  ├── internal/dma tests (MockDescriptor) │
│  ├── sync/ tests                         │
│  └── integration/smoltcp tests           │
│                                          │
│  cargo clippy (feature matrix)           │
│  cargo doc --no-deps                     │
│  cargo +esp check (embedded target)      │
└──────────────────────────────────────────┘

┌──────────────────────────────────────────┐
│           On-Device (Manual)             │
│                                          │
│  cargo xtask run qa-runner               │
│  ├── Group 1: Register access            │
│  ├── Group 2: EMAC initialization        │
│  ├── Group 3: PHY communication          │
│  ├── Group 4: EMAC operations            │
│  ├── Group 5: Link status                │
│  ├── Group 6: smoltcp integration        │
│  ├── Group 7: State & interrupts         │
│  ├── Group 8: Advanced features          │
│  └── Group 9: Edge cases                 │
└──────────────────────────────────────────┘
```

### Mock Utilities

The `src/testing/` module (available only under `#[cfg(test)]`) provides:

| Mock | Purpose |
|------|---------|
| `MockMdioBus` | Simulates PHY register reads/writes with write logging |
| `MockDelay` | Tracks delay calls without sleeping |
| `MockDescriptor` | Simulates DMA descriptor state transitions |

Helper macros:

| Macro | Purpose |
|-------|---------|
| `assert_reg_written!` | Assert a specific PHY register write occurred |
| `assert_reg_written_any!` | Assert any write to a PHY register occurred |

### Planned Improvements

#### Per-Feature CI Test Jobs

Run `cargo test --lib` under each optional feature individually to catch
conditional compilation issues:

```yaml
strategy:
  matrix:
    features:
      - ""
      - "smoltcp"
      - "async,critical-section"
      - "embassy-net,critical-section"
      - "smoltcp,async,critical-section,embassy-net"
```

#### Additional Host Mocks

- **`MockEmac`**: A lightweight `Emac` stub that does not require hardware
  register access. This would allow integration-layer tests to exercise the
  full `Device::receive` / `Driver::transmit` token flow on the host.
- **Error injection in `MockMdioBus`**: Configurable failure modes (timeout,
  bus error) for resilience testing.
- **Configurable `MockDelay`**: Simulate wall-clock elapsed time to test
  timeout and polling logic in `EmacPhyBundle::wait_link_up`.

#### Hardware-in-the-Loop (HIL) CI

A self-hosted GitHub Actions runner connected to a WT32-ETH01 board can
automate on-device validation:

```text
┌────────────┐     ┌───────────────┐     ┌────────────┐
│  GitHub PR  │────▶│  Self-hosted   │────▶│  WT32-ETH01│
│  (ci.yml)   │     │  Runner        │     │  Board     │
└────────────┘     └───────────────┘     └────────────┘
                          │
                          ▼
                    ┌─────────────┐
                    │  qa-runner   │
                    │  serial log  │
                    └─────────────┘
```

Steps:
1. Build and flash `qa-runner` via `cargo xtask run qa-runner`.
2. Capture serial output and parse pass/fail verdicts.
3. Upload logs as GitHub Actions artifacts.
4. Block PR merge on failure.

#### Coverage Gating

- Run coverage with each feature combination to measure conditional code.
- Set a minimum line-coverage threshold (target: 80% for host-testable code).
- Track coverage trends and alert on regressions.

---

## Notes for Production Use

If you are shipping to production or critical devices, validate on the exact
board/PHY and network environment you plan to deploy. Treat hardware validation
as required, not optional.
