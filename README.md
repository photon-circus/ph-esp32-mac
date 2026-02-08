# ph-esp32-mac

`ph-esp32-mac` is a `no_std`, `no_alloc` Rust driver for the ESP32 Ethernet MAC (EMAC). It targets ESP32 hardware, provides LAN8720A PHY support, and integrates with smoltcp, embassy-net, and esp-hal.

---

## Table of Contents

1. [Overview](#overview)
2. [Status & Scope](#status--scope)
3. [Motivation](#motivation)
4. [Goal](#goal)
5. [Features](#features)
6. [Supported Hardware](#supported-hardware)
7. [Quick Start (Happy Path)](#quick-start-happy-path)
8. [Recommended Workflow](#recommended-workflow)
9. [Examples](#examples)
10. [Memory & DMA Sizing](#memory--dma-sizing)
11. [Feature Flags](#feature-flags)
12. [MSRV](#msrv)
13. [Documentation](#documentation)
14. [License](#license)

---

## Overview

This crate implements the ESP32 EMAC peripheral using static DMA descriptors and buffers. It is designed for embedded use with explicit initialization, no heap allocation, and predictable memory usage. The public API is optimized for esp-hal consumers while keeping low-level control available.

---

## Status & Scope

- **Current target**: ESP32 (this release only)
- **ESP32-P4**: Experimental placeholder (not implemented, hidden from docs)
- **Happy path**: esp-hal synchronous and async bring-up on WT32-ETH01

---

## Motivation

There was no existing bare-metal, `no_std`, `no_alloc` Rust driver for the ESP32 MAC, which created a barrier to using `wt32_eth01.rs` in the bare-metal ecosystem.

---

## Goal

Provide an efficient bare-metal, `no_std`, `no_alloc` implementation of the ESP32 MAC to enable use of the LAN8720A Ethernet PHY.

---

## Features

- ESP32 RMII support (MII is available but secondary)
- LAN8720A PHY driver with a generic PHY fallback
- smoltcp integration (`smoltcp` feature)
- embassy-net driver integration (`embassy-net` feature)
- esp-hal integration helpers (`esp-hal` feature)
- Async/waker support without allocation (`async` + `critical-section`)

---

## Supported Hardware

- **ESP32** (current release target)
- **ESP32-P4** (experimental / not implemented yet, hidden from docs)

---

## Quick Start (Happy Path)

These are the recommended esp-hal bring-up paths and match the examples in
`apps/examples/`.

### esp-hal (sync)

```rust
use esp_hal::delay::Delay;
use ph_esp32_mac::esp_hal::{EmacBuilder, EmacPhyBundle, Wt32Eth01};

ph_esp32_mac::emac_static_sync!(EMAC, 10, 10, 1600);

let mut delay = Delay::new();
EMAC.with(|emac| {
    EmacBuilder::wt32_eth01_with_mac(emac, [0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
        .init(&mut delay)
        .unwrap();
    let mut emac_phy = EmacPhyBundle::wt32_eth01_lan8720a(emac, Delay::new());
    let _status = emac_phy
        .init_and_wait_link_up(&mut delay, 10_000, 200)
        .unwrap();
    emac.start().unwrap();
});
```

### esp-hal (async)

```rust
use esp_hal::delay::Delay;
use ph_esp32_mac::esp_hal::{EmacBuilder, EmacPhyBundle, Wt32Eth01};
use ph_esp32_mac::{emac_async_isr, emac_static_async};

emac_static_async!(EMAC, EMAC_STATE, 10, 10, 1600);
emac_async_isr!(EMAC_IRQ, esp_hal::interrupt::Priority::Priority1, &EMAC_STATE);

let mut delay = Delay::new();
let emac_ptr = EMAC.init(ph_esp32_mac::Emac::new()) as *mut _;
unsafe {
    EmacBuilder::wt32_eth01_with_mac(&mut *emac_ptr, [0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
        .init(&mut delay)
        .unwrap();
    let mut emac_phy = EmacPhyBundle::wt32_eth01_lan8720a(&mut *emac_ptr, Delay::new());
    let _status = emac_phy
        .init_and_wait_link_up(&mut delay, 10_000, 200)
        .unwrap();
    (*emac_ptr).start().unwrap();
}
```

---

## Recommended Workflow

From the repo root, use `cargo xtask` to build and flash apps:

```bash
cargo xtask run ex-esp-hal
cargo xtask run ex-esp-hal-async
```

See [xtask/README.md](xtask/README.md) for details.

---

## Examples

Examples are provided as a **separate crate** in this repository and are not
packaged with the published library crate.
They require the Xtensa toolchain and flashing setup, so they are kept repo-only
to avoid pulling those requirements into crates.io consumers.

See [apps/examples/README.md](apps/examples/README.md) for build and run
instructions.

Recommended runner (from repo root):
```bash
cargo xtask run ex-embassy-net
```

Included examples:

- `smoltcp_echo`
- `esp_hal_integration`
- `esp_hal_async`
- `embassy_net`

Hardware QA runner (separate crate):
- [apps/qa-runner/README.md](apps/qa-runner/README.md)
- `cargo xtask run qa-runner`

---

## Memory & DMA Sizing

Default configuration (10 RX/TX buffers, 1600 bytes each):

| Component | Size |
|-----------|------|
| RX descriptors | 320 bytes |
| TX descriptors | 320 bytes |
| RX buffers | 16,000 bytes |
| TX buffers | 16,000 bytes |
| **Total** | **~32 KB** |

Adjust via the static macros if memory is constrained:

```rust
ph_esp32_mac::emac_static_sync!(EMAC, 4, 4, 1600);
```

---

## Feature Flags

| Feature | Description |
|---------|-------------|
| `esp32` | ESP32 target (default) |
| `esp32p4` | Experimental placeholder (not implemented) |
| `smoltcp` | smoltcp integration |
| `embassy-net` | embassy-net-driver integration |
| `esp-hal` | esp-hal integration helpers |
| `critical-section` | Shared/ISR-safe access wrappers |
| `async` | Async/waker support (requires `critical-section`) |
| `defmt` | defmt formatting support |
| `log` | log crate support |

---

## MSRV

- Rust **1.92.0**

---

## Documentation

- [docs/README.md](docs/README.md) (documentation index and TOC)
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)
- [SECURITY.md](SECURITY.md)
- [CHANGELOG.md](CHANGELOG.md)

---

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
