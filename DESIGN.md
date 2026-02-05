# ESP32 EMAC Design

This document describes the current architecture and behavior of `ph-esp32-mac`.
Forward-looking work and scope changes are tracked in `PUBLISHABILITY_ROADMAP.md`
and take precedence if anything diverges.

## Table of Contents

1. [Scope](#scope)
2. [Goals](#goals)
3. [Architecture Overview](#architecture-overview)
4. [Module Layout](#module-layout)
5. [Core Driver Model](#core-driver-model)
6. [Memory Model](#memory-model)
7. [Hardware Access](#hardware-access)
8. [PHY Layer](#phy-layer)
9. [Integration Layers](#integration-layers)
10. [Feature Flags](#feature-flags)
11. [Testing](#testing)
12. [References](#references)

---

## Scope

- Target: ESP32 only (xtensa-esp32-none-elf).
- The `esp32p4` feature exists as an experimental placeholder for internal
  layout work. It is not supported in this release and is intentionally
  undocumented.
- `no_std`, `no_alloc`; all buffers are statically allocated.

## Goals

- Provide a safe, zero-allocation EMAC driver for ESP32.
- Keep the public API small, stable, and HAL-friendly.
- Isolate hardware register access to internal modules.
- Support optional integrations (smoltcp, embassy-net-driver, esp-hal).

---

## Architecture Overview

```
+--------------------------------------------------------------+
| Application Layer                                            |
| smoltcp / embassy-net / raw Ethernet processing               |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| Integration Layer                                             |
| integration/{smoltcp, embassy_net, esp_hal}                   |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| Sync Layer                                                    |
| sync::{SharedEmac, AsyncEmacState, AsyncEmacExt}              |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| Driver Layer                                                  |
| driver::{emac, config, interrupt, filtering, flow}            |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| HAL Layer                                                     |
| hal::{clock, reset, mdio, gpio}                               |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| Internal Layer                                                |
| internal::{register, dma, phy_regs, constants}                |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| ESP32 Hardware                                                |
| EMAC MAC + DMA + EXT + External PHY                           |
+--------------------------------------------------------------+
```

---

## Module Layout

```
src/
├── driver/           # Core driver: Emac, config, errors, filtering, flow
├── hal/              # Clock/reset/MDIO/GPIO abstractions
├── phy/              # PHY drivers and traits (Lan8720a, generic helpers)
├── integration/      # smoltcp, embassy-net-driver, esp-hal facades
├── sync/             # SharedEmac + async waker support
├── internal/         # Registers, DMA descriptors, constants (pub(crate))
├── testing/          # Host test utilities (cfg(test))
└── lib.rs            # Public API and re-exports
```

Public API is surfaced via `lib.rs` re-exports. Anything inside `internal/`
is implementation detail and may change without notice.

---

## Core Driver Model

- `Emac<const RX, const TX, const BUF>` is the main driver type.
- `EmacConfig` provides a builder-style configuration API.
- `EmacConfig::rmii_esp32_default()` encodes the RMII defaults for ESP32.
- `InterruptStatus` provides typed interrupt parsing.
- Filtering and flow control live in `driver::filtering` and `driver::flow`.

Driver lifecycle is explicit:

```
Created -> Initialized -> Running
```

---

## Memory Model

The driver is `no_alloc` and uses const generics for buffer sizing. Static
allocation is required because DMA needs stable, DMA-capable memory.

Type aliases provide common presets:

- `EmacSmall`
- `EmacDefault`
- `EmacLarge`

Place EMAC instances in DMA-capable SRAM (via a linker section if required by
your memory map).

---

## Hardware Access

All register access is contained in `internal/register/*` and re-exported
as `DmaRegs`, `MacRegs`, and `ExtRegs` for internal use. This keeps unsafe
volatile access centralized and consistent.

DMA descriptors are implemented under `internal/dma/descriptor` and are aligned
for ESP32. The experimental `esp32p4` feature changes descriptor alignment, but
that target is not supported in this release.

---

## PHY Layer

The PHY layer is trait-driven and uses `hal::mdio::MdioBus` for register access.

Key types:

- `PhyDriver` trait for PHY implementations
- `LinkStatus` for speed/duplex reporting
- `Lan8720a` and `Lan8720aWithReset` as the primary PHYs

The driver does not hardcode a PHY; integration layers can use helpers like
`EmacPhyBundle` (esp-hal facade) to reduce boilerplate.

---

## Integration Layers

### smoltcp

`integration::smoltcp` implements `smoltcp::phy::Device` for `Emac`. The design
uses short-lived RX/TX tokens with internal raw pointers to satisfy smoltcp's
API requirements while keeping safety guarantees localized.

### esp-hal

`integration::esp_hal` provides an ergonomic facade:

- `EmacBuilder` for HAL-friendly construction
- `EmacPhyBundle` for PHY init and link polling
- `EmacExt` for interrupt binding
- `emac_isr!` and `emac_async_isr!` macros for handler setup

### async

`sync::asynch` provides per-instance wakers and async TX/RX:

- `AsyncEmacState` holds RX/TX/error wakers
- `AsyncEmacExt` adds `receive_async()` and `transmit_async()`
- `async_interrupt_handler(state)` wakes tasks from the ISR

### embassy-net

`integration::embassy_net` implements `embassy-net-driver` (driver-only). The
wrapper type `EmbassyEmac` and `EmbassyEmacState` integrate with Embassy stacks
without depending on `embassy-net` directly.

---

## Feature Flags

```
[features]
- esp32 (default)            : target ESP32
- esp32p4 (experimental)     : placeholder only, not supported
- defmt                      : defmt formatting
- log                        : log facade support
- smoltcp                    : smoltcp Device integration
- critical-section           : SharedEmac + critical-section primitives
- async                      : async wakers and async TX/RX
- esp-hal                    : esp-hal facade helpers
- embassy-net                : embassy-net-driver integration
```

---

## Testing

Testing strategy, coverage goals, and integration test guidance live in
`TESTING.md`.

---

## References

- ESP32 Technical Reference Manual (Ethernet MAC chapter)
- Synopsys DesignWare MAC documentation
- LAN8720A datasheet
- embedded-hal 1.0 documentation
- smoltcp 0.12 documentation
