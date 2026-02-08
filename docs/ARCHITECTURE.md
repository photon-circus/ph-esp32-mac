# Architecture Overview

This document provides a high-level architectural view of `ph-esp32-mac`.
For detailed design notes and implementation choices, see `docs/DESIGN.md`.
Forward-looking changes are tracked in `PUBLISHABILITY_ROADMAP.md`.

Last updated: 2026-02-05

---

## Scope

- Target: **ESP32** only (xtensa-esp32-none-elf).
- `esp32p4` exists as an **experimental placeholder** and is hidden from docs.
- `no_std`, `no_alloc`; all buffers are statically allocated.
- Primary consumer: `esp-hal` 1.0.0.

---

## Layered Architecture

```
┌────────────────────────────────────────────────────────────┐
│ Application                                                │
│ smoltcp / embassy-net / raw Ethernet                       │
└────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────┐
│ Integration                                                │
│ integration::{esp_hal, smoltcp, embassy_net}               │
└────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────┐
│ Concurrency + ISR Safety                                   │
│ sync::{SharedEmac, AsyncEmacState, AsyncEmacExt}           │
└────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────┐
│ Driver Core                                                │
│ driver::{emac, config, interrupt, filtering, flow}         │
└────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────┐
│ HAL / Bring-up                                             │
│ hal::{clock, reset, mdio}                                  │
└────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────┐
│ Internal                                                   │
│ internal::{register, dma, constants}                       │
└────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌────────────────────────────────────────────────────────────┐
│ ESP32 Hardware                                             │
│ EMAC MAC + DMA + EXT + External PHY                        │
└────────────────────────────────────────────────────────────┘
```

---

## Module Responsibilities

```
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
  feature-extensions (filtering, flow control).
- **phy**: trait-based PHY layer and LAN8720A implementation.
- **boards**: opinionated helpers (WT32-ETH01) that define a canonical happy
  path for esp-hal users.
- **integration**: interface adapters for common stacks and runtimes.
- **sync**: ISR-safe shared access and async waker-driven IO.
- **hal/internal**: low-level register access and DMA mechanics.

---

## Data Flow

### RX Path

1. DMA writes frame into a descriptor-owned buffer.
2. `Emac::receive()` or `AsyncEmacExt::receive_async()` pulls the frame.
3. Integration layers (smoltcp/embassy-net) expose tokens or driver traits.

### TX Path

1. Caller provides a frame to `Emac::transmit()` or async TX.
2. DMA descriptor is populated and handed to hardware.
3. Completion is signaled via interrupt and async wakers (if enabled).

---

## Concurrency and Interrupt Model

The driver exposes **ISR-safe** access via shared wrappers:

- `SharedEmac` uses a `critical-section` backend for safe access from main + ISR.
- `AsyncEmacState` stores RX/TX/error wakers per instance.
- `async_interrupt_handler` is called from the ISR to wake tasks.

This keeps the driver `no_std`/`no_alloc` while still supporting async use.

---

## Memory Model

All DMA buffers and descriptors are **static** and **DMA-capable**:

- Const generics define RX/TX ring sizes and buffer size.
- Aliases `EmacSmall`, `EmacDefault`, `EmacLarge` provide presets.
- ESP32 requires DMA buffers in SRAM; linkers/sections are used as needed.

---

## PHY Bring-up

The PHY layer is MDIO-driven and trait-based:

- `PhyDriver` defines the PHY contract.
- `Lan8720a` is the canonical PHY implementation.
- `EmacPhyBundle` (esp-hal facade) performs init + link polling.

---

## Integration Facades

- **esp-hal**: ergonomic builders/macros for the canonical bring-up path.
- **smoltcp**: implements `smoltcp::phy::Device` for `Emac`.
- **embassy-net**: implements `embassy-net-driver` for `Emac`.

The facades are opinionated to reduce boilerplate while keeping the core driver
flexible for bare-metal users.

---

## Feature Flags

```
esp32 (default)      Target ESP32
esp32p4 (hidden)     Experimental placeholder
critical-section     SharedEmac + ISR safety
async                Async waker support
esp-hal              esp-hal facade helpers
smoltcp              smoltcp Device integration
embassy-net          embassy-net-driver integration
defmt/log            Optional logging backends
```

---

## Related Documents

- `docs/DESIGN.md` – detailed design notes
- `docs/TESTING.md` – test strategy and coverage
- `docs/API.md` – public API inventory and stability
- `PUBLISHABILITY_ROADMAP.md` – sprint plan and publishability status
