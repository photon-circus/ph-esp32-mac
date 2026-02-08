# Testing

This document summarizes how to validate the driver and where the testing gaps
still exist. The project is usable, but **hardware correctness remains the
highest-risk area** and depends on real-device validation.

---

## Quick Checks (Host)

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

## Hardware Validation (Recommended)

Integration tests live in `apps/qa-runner/` and are the primary way to
exercise DMA, register access, and PHY behavior.

From the repo root:

```bash
cargo xtask build qa-runner
cargo xtask run qa-runner
```

See `apps/qa-runner/README.md` for wiring and board notes.

---

## What We Cover Well

- Configuration, error handling, and parsing logic (host unit tests)
- PHY logic via MDIO mocks (host tests)
- Core DMA ring behavior with mocked descriptors (host tests)

---

## Known Gaps and Risks

- **Hardware-only paths**: register access, clock/reset sequencing, and DMA
  ownership are validated primarily on hardware.
- **Network variability**: DHCP and link bring-up can be sensitive to timing and
  network environment; false negatives are possible without traffic.
- **Async runtime behavior**: waker logic is unit-tested, but full async behavior
  depends on the target runtime and is best validated on device.

If you are shipping to production or critical devices, **treat the driver as
“requires hardware validation”** and verify on the exact board/PHY you plan to
deploy.
