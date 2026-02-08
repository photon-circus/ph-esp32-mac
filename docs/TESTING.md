# Testing

This document summarizes how to validate the driver and where the testing gaps
remain. Hardware correctness is the highest-risk area and requires on-device
validation.

---

## Table of Contents

- [Host Checks](#host-checks)
- [Hardware Validation](#hardware-validation)
- [Coverage and Gaps](#coverage-and-gaps)
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

## Coverage and Gaps

Covered well:
- Configuration, error handling, and parsing logic (host unit tests)
- PHY logic via MDIO mocks (host tests)
- Core DMA ring behavior with mocked descriptors (host tests)

Known gaps and risks:
- Hardware-only paths: register access, clock/reset sequencing, DMA ownership
- Network variability: DHCP and link bring-up depend on external traffic
- Async runtime behavior: waker logic is unit-tested, full behavior is on-target

---

## Notes for Production Use

If you are shipping to production or critical devices, validate on the exact
board/PHY and network environment you plan to deploy. Treat hardware validation
as required, not optional.
