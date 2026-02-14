# Architecture

This document provides a high-level architectural view of the `ph-esp32-mac`
driver, its layers, and the main data flows.

---

## Table of Contents

- [Scope](#scope)
- [Layered Architecture](#layered-architecture)
- [Module Responsibilities](#module-responsibilities)
- [Data Flow](#data-flow)
- [Concurrency and Interrupts](#concurrency-and-interrupts)
- [Memory Model](#memory-model)
- [Integration Facades](#integration-facades)
- [Related Documents](#related-documents)

---

## Scope

- Target: ESP32 only (`xtensa-esp32-none-elf`).
- `no_std`, `no_alloc`; all buffers are statically allocated.
- App crates live under `apps/` and are built via `cargo xtask`.

---

## Layered Architecture

```text
+----------------------------------------------------------+
| Application                                              |
| smoltcp / embassy-net / raw Ethernet processing          |
+----------------------------------------------------------+
                           |
                           v
+----------------------------------------------------------+
| Integration Facades                                     |
| integration::{esp_hal, smoltcp, embassy_net}             |
+----------------------------------------------------------+
                           |
                           v
+----------------------------------------------------------+
| Concurrency + ISR Safety                                |
| sync::{SharedEmac, AsyncEmacState, AsyncEmacExt}         |
+----------------------------------------------------------+
                           |
                           v
+----------------------------------------------------------+
| Driver Core                                              |
| driver::{emac, config, interrupt, filtering, flow}       |
+----------------------------------------------------------+
                           |
                           v
+----------------------------------------------------------+
| HAL / Bring-up                                           |
| hal::{clock, reset, mdio}                                |
+----------------------------------------------------------+
                           |
                           v
+----------------------------------------------------------+
| Internal                                                 |
| internal::{register, dma, phy_regs, constants}           |
+----------------------------------------------------------+
                           |
                           v
+----------------------------------------------------------+
| ESP32 Hardware                                           |
| EMAC MAC + DMA + EXT + External PHY                      |
+----------------------------------------------------------+
```

---

## Module Responsibilities

```text
src/
├── driver/       Core EMAC API and configuration
├── phy/          PHY trait + LAN8720A driver
├── boards/       Board-specific helpers (WT32-ETH01)
├── integration/  esp-hal / smoltcp / embassy-net facades
├── sync/         SharedEmac and async waker support
├── hal/          Clock/reset/MDIO bring-up helpers
└── internal/     Registers, DMA descriptors, constants
```

- **driver**: exposes `Emac` and its configuration, interrupt handling, and
  feature extensions (filtering, flow control).
- **phy**: trait-based PHY layer and LAN8720A implementation.
- **boards**: opinionated helpers for a canonical esp-hal path.
- **integration**: adapters for common stacks and runtimes.
- **sync**: ISR-safe shared access and async waker-driven I/O.
- **hal**: clock/reset/MDIO bring-up helpers.
- **internal**: register access, DMA descriptors, and constants.

---

## Data Flow

### RX Path

1. DMA writes a frame into a descriptor-owned buffer.
2. `Emac::receive()` or `AsyncEmacExt::receive_async()` pulls the frame.
3. Integration layers expose tokens or driver traits.

### TX Path

1. Caller provides a frame to `Emac::transmit()` or async TX.
2. DMA descriptor is populated and handed to hardware.
3. Completion is signaled via interrupt and async wakers (if enabled).

---

## Concurrency and Interrupts

The driver exposes ISR-safe access via shared wrappers:

- `SharedEmac` uses `critical-section` to synchronize main + ISR access.
- `AsyncEmacState` stores RX/TX/error wakers per instance.
- `async_interrupt_handler` is called from the ISR to wake tasks.

---

## Memory Model

All DMA buffers and descriptors are static and DMA-capable:

- Const generics define RX/TX ring sizes and buffer size.
- Aliases `EmacSmall`, `EmacDefault`, `EmacLarge` provide presets.
- ESP32 requires DMA buffers in SRAM; linker sections are used as needed.

---

## Integration Facades

- **esp-hal**: ergonomic builders/macros for the canonical bring-up path.
- **smoltcp**: implements `smoltcp::phy::Device` for `Emac`.
- **embassy-net**: implements `embassy-net-driver` for `Emac`.

The core driver remains runtime-agnostic; these facades are optional.

---

## Related Documents

- [DESIGN.md](DESIGN.md) - design decisions and constraints
- [TESTING.md](TESTING.md) - test strategy and known gaps
- [ROADMAP.md](ROADMAP.md) - feature roadmap and planned improvements
