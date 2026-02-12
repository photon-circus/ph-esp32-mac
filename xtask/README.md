# xtask

Helper utility for building and flashing ESP32 app crates under `apps/`. This
crate is not published to crates.io.

---

## Table of Contents

- [Overview](#overview)
- [Usage](#usage)
- [Targets](#targets)
- [Notes](#notes)

---

## Overview

`xtask` resolves a short target name (or `.rs` path) to a Cargo binary, injects
the Xtensa target and linker flags, and runs the build through the ESP toolchain.

---

## Usage

Run from the repo root:

```bash
cargo xtask run ex-smoltcp
cargo xtask build qa-runner
```

Debug build:

```bash
cargo xtask run ex-embassy-net --debug
```

Pass args to the target:

```bash
cargo xtask run ex-esp-hal -- --extra-arg
```

---

## Targets

| Target | Resolves To |
|--------|-------------|
| `qa-runner`, `qa` | `apps/qa-runner/qa_runner.rs` |
| `ex-esp-hal` | `apps/examples/esp_hal_integration.rs` |
| `ex-esp-hal-async` | `apps/examples/esp_hal_async.rs` |
| `ex-smoltcp` | `apps/examples/smoltcp_echo.rs` |
| `ex-embassy`, `ex-embassy-net` | `apps/examples/embassy_net.rs` |

---

## Notes

- If no command is supplied, `build` is assumed.
- `--debug` selects a debug build (release is the default).
- `--` passes arguments to the target binary.
- `ESP_LOG`, `ESP_IDF_VERSION`, and `CARGO_TARGET_DIR` are defaulted if unset.
