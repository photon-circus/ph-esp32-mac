# Examples

This directory contains ESP32 example apps for `ph-esp32-mac`. Each example is
documented in its source file (see the module-level docs at the top).

## Quick Overview

| Example | Summary |
|---------|---------|
| `esp_hal_integration.rs` | Minimal esp-hal synchronous bring-up |
| `esp_hal_async.rs` | Async RX with per-instance wakers |
| `smoltcp_echo.rs` | TCP echo server using smoltcp |
| `embassy_net.rs` | Async networking using embassy-net |

## Prerequisites

- Install the ESP toolchain (`espup install`).
- Install `espflash` for flashing/monitoring.

## Running With `cargo xtask` (Recommended)

Run from the repo root. `xtask` selects the right crate, target, and features,
injects the required linker flags, and invokes the ESP toolchain (`rustup run esp`)
with `-Zbuild-std=core` for the Xtensa target.

```bash
cargo xtask run ex-esp-hal
cargo xtask run ex-esp-hal-async
cargo xtask run ex-smoltcp
cargo xtask run ex-embassy-net
```

Build only (no flash):

```bash
cargo xtask build ex-smoltcp
```

Debug build:

```bash
cargo xtask run ex-smoltcp --debug
```

You can also pass a `.rs` entry path instead of a short name.

Environment overrides:

```bash
$env:ESPFLASH_PORT = "COM7"
$env:ESPFLASH_BAUD = "921600"
```

## Hardware Defaults

All examples default to the WT32-ETH01 board (LAN8720A, external 50 MHz clock).
If you use different hardware, adjust the constants at the top of the example.

## Troubleshooting

- **DHCP takes too long or never assigns**: confirm link is up, give it 10–30s,
  and ensure your network allows broadcast/multicast. If still stuck, power-cycle
  the board and retry.

## Related

- `apps/qa-runner/` for hardware QA runs
- `docs/TESTING.md` for test strategy
