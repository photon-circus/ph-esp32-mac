# Testing Documentation

This document describes how to validate the driver on host and on hardware.
For forward-looking work or scope changes, refer to `PUBLISHABILITY_ROADMAP.md`.

## Table of Contents

1. [Testing Strategy](#testing-strategy)
2. [Coverage Guidance](#coverage-guidance)
3. [Running Tests](#running-tests)
4. [Test Utilities](#test-utilities)
5. [Known Limitations](#known-limitations)

---

## Testing Strategy

We use a two-tier approach:

| Tier | Environment | Purpose |
|------|-------------|---------|
| Unit tests | Host (x86/ARM) | Validate logic, configuration, and parsing without hardware |
| Integration tests | ESP32 hardware | Validate registers, DMA, and PHY behavior on real devices |

Key principles:

- Test as much logic as possible on the host.
- Use mocks for MDIO and timing in unit tests.
- Reserve hardware testing for DMA, PHY, and register behavior.

---

## Coverage Guidance

Coverage targets are relative and depend on testability:

| Code Area | Target Coverage | Notes |
|-----------|-----------------|-------|
| Config / errors / parsing | 80%+ | Pure logic, easy to test |
| PHY and DMA logic | 65%+ | Uses mocks, moderate complexity |
| Register access / clocks | 0% | Hardware-only, validated via integration tests |

Avoid hard-coding coverage numbers in docs; use `cargo llvm-cov` for current data.

---

## Running Tests

### Host Unit Tests

```bash
# Run all host unit tests
cargo test --lib

# Run a specific module's tests
cargo test --lib phy::lan8720a

# Coverage report (requires cargo-llvm-cov)
cargo llvm-cov --lib
```

Useful aliases from `.cargo/config.toml`:

- `cargo t` (test --lib)
- `cargo cov` (llvm-cov --lib)
- `cargo c` (clippy -- -D warnings)

### Integration Tests (Hardware)

Integration tests live in `integration_tests/` and require the ESP32 toolchain
and flashing tools. From the project root:

```bash
# Build, flash, and monitor
cargo int

# Build only
cargo int-build
```

See `integration_tests/README.md` for hardware setup, wiring, and troubleshooting.

---

## Test Utilities

Host tests use mocks provided in `src/testing`:

- `MockMdioBus` for PHY register simulation
- `MockDelay` for timing verification
- `MockDescriptor` for DMA ring logic
- `assert_reg_written` and `assert_reg_written_any` macros

---

## Known Limitations

- Register access, clock setup, and reset sequences are hardware-only and are
  validated by integration tests instead of unit tests.
- Some integration tests depend on external network traffic; "no traffic"
  warnings are expected on quiet networks.
- Async and embassy-net behavior depends on a runtime; unit tests validate
  waker logic and API availability, while full behavior is exercised in
  examples and hardware tests.
