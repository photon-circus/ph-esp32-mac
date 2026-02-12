# Apps

Standalone application crates for hardware verification and usage examples. These
crates are not published to crates.io.

---

## Table of Contents

- [Overview](#overview)
- [Structure](#structure)
- [Running With `cargo xtask`](#running-with-cargo-xtask)
- [Notes](#notes)

---

## Overview

The `apps/` directory contains crates that exercise the driver on real ESP32
hardware. They are intentionally kept out of the main crate to avoid host
build requirements.

---

## Structure

| Path | Purpose |
|------|---------|
| `apps/examples/` | Example applications (esp-hal, smoltcp, embassy-net) |
| `apps/qa-runner/` | Hardware QA runner for WT32-ETH01 |

---

## Running With `cargo xtask`

Run from the repo root:

```bash
cargo xtask run ex-embassy-net
cargo xtask run ex-smoltcp
cargo xtask run qa-runner
```

---

## Notes

See `apps/examples/README.md` and `apps/qa-runner/README.md` for details and
hardware setup.
