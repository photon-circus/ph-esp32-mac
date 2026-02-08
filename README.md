# ph-esp32-mac

`ph-esp32-mac` is a `no_std`, `no_alloc` Rust driver for the ESP32 Ethernet MAC (EMAC). It targets ESP32 hardware, provides LAN8720A PHY support, and integrates with smoltcp, embassy-net, and esp-hal.

---

## Table of Contents

1. [Overview](#overview)
2. [Motivation](#motivation)
3. [Goal](#goal)
4. [Features](#features)
5. [Supported Hardware](#supported-hardware)
6. [Quick Start](#quick-start)
7. [Ergonomic Helpers (esp-hal)](#ergonomic-helpers-esp-hal)
8. [Examples](#examples)
9. [Feature Flags](#feature-flags)
10. [MSRV](#msrv)
11. [Documentation](#documentation)
12. [License](#license)

---

## Overview

This crate implements the ESP32 EMAC peripheral using static DMA descriptors and buffers. It is designed for embedded use with explicit initialization, no heap allocation, and predictable memory usage. The public API is optimized for esp-hal consumers while keeping low-level control available.

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

## Quick Start

```rust
use ph_esp32_mac::{Emac, EmacConfig, Lan8720a, MdioController, PhyDriver, PhyInterface, RmiiClockMode};
use embedded_hal::delay::DelayNs;

// Static allocation is required for DMA descriptors.
static mut EMAC: Emac<10, 10, 1600> = Emac::new();

// Your delay implementation (from esp-hal or custom).
let mut delay = /* DelayNs impl */;

let emac = unsafe { &mut EMAC };
let config = EmacConfig::rmii_esp32_default()
    .with_mac_address([0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
    .with_phy_interface(PhyInterface::Rmii)
    .with_rmii_clock(RmiiClockMode::ExternalInput { gpio: 0 });

emac.init(config, &mut delay)?;

let mut mdio = MdioController::new(delay);
let mut phy = Lan8720a::new(1);
phy.init(&mut mdio)?;

emac.start()?;
# Ok::<(), ph_esp32_mac::Error>(())
```

---

## Ergonomic Helpers (esp-hal)

For esp-hal users, the crate provides opinionated helpers and macros for the
WT32-ETH01 “happy path” to reduce boilerplate:

```rust
use esp_hal::delay::Delay;
use ph_esp32_mac::esp_hal::{EmacBuilder, EmacPhyBundle, Wt32Eth01};

ph_esp32_mac::emac_static_sync!(EMAC);

let mut delay = Delay::new();
EMAC.with(|emac| {
    EmacBuilder::wt32_eth01_with_mac(emac, [0x02, 0x00, 0x00, 0x12, 0x34, 0x56])
        .init(&mut delay)
        .unwrap();
    let mut emac_phy = EmacPhyBundle::wt32_eth01_lan8720a(emac, Delay::new());
    let _status = emac_phy.init_and_wait_link_up(&mut delay, 10_000, 200).unwrap();
    emac.start().unwrap();
});
```

---

## Examples

Examples are provided as a **separate crate** in this repository and are not
packaged with the published library crate.

See the examples for build and run instructions:
https://github.com/photon-circus/ph-esp32-mac/tree/main/apps/examples

Recommended runner (from repo root):
```bash
cargo xtask run ex-embassy
```

Included examples:

- `smoltcp_echo`
- `esp_hal_integration`
- `esp_hal_async`
- `embassy_net`

Hardware QA runner (separate crate):
- https://github.com/photon-circus/ph-esp32-mac/tree/main/apps/qa-runner
- `cargo xtask run qa-runner`

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

- [DESIGN.md](docs/DESIGN.md)
- [TESTING.md](docs/TESTING.md)
- [DOCUMENTATION_STANDARDS.md](docs/DOCUMENTATION_STANDARDS.md)

---

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
