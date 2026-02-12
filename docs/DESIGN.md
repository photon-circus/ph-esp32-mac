# Design

This document captures design decisions, constraints, and invariants for the
`ph-esp32-mac` driver. It focuses on why the system is shaped the way it is.

---

## Table of Contents

- [Scope and Non-Goals](#scope-and-non-goals)
- [Design Principles](#design-principles)
- [Driver Model](#driver-model)
- [DMA and Memory Invariants](#dma-and-memory-invariants)
- [Safety Boundaries](#safety-boundaries)
- [PHY Strategy](#phy-strategy)
- [Integration Strategy](#integration-strategy)
- [Board Support](#board-support)
- [Feature Flags](#feature-flags)

---

## Scope and Non-Goals

Scope:
- ESP32 only (`xtensa-esp32-none-elf`)
- `no_std`, `no_alloc`, statically allocated DMA buffers
- LAN8720A as the canonical PHY

Non-goals:
- WiFi support (out of scope)
- Dynamic allocation or runtime buffer growth
- Stable support for non-ESP32 targets (ESP32-P4 is a placeholder only)

---

## Design Principles

- **Predictable memory usage**: const generics and static allocation.
- **Explicit lifecycle**: `Emac::new` -> `init` -> `start`.
- **Minimal unsafe surface**: unsafe is isolated to internal modules.
- **HAL-friendly**: ergonomic facades for esp-hal without hiding core control.
- **Runtime-agnostic**: optional integrations for smoltcp/embassy-net.

---

## Driver Model

The driver centers on `Emac<RX, TX, BUF>` plus `EmacConfig`:

- `EmacConfig::rmii_esp32_default()` captures sensible ESP32 defaults.
- Configuration is explicit and builder-style for clarity.
- Errors are typed and recoverable where possible.

---

## DMA and Memory Invariants

- DMA descriptors and buffers are static and DMA-capable.
- RX/TX rings are fixed-size and circular.
- CPU and DMA ownership of descriptors is exclusive at any moment.
- Buffer sizes must be large enough for expected frames (typically 1600 bytes).

---

## Safety Boundaries

Unsafe access is concentrated in `internal/`:

- Register reads/writes are isolated in `internal/register/*`.
- DMA descriptor manipulation is isolated in `internal/dma/*`.
- Public APIs provide safe abstractions and validate input where possible.

Macro helpers (`emac_static_*`) place critical buffers in DMA-capable memory
when targeting Xtensa.

---

## PHY Strategy

The PHY layer is trait-based:

- `PhyDriver` defines the contract.
- `Lan8720a` is the reference implementation.
- A generic PHY fallback supports basic link operations.

This keeps the driver compatible with other PHYs without hardcoding one.

---

## Integration Strategy

Integrations are optional and additive:

- **esp-hal**: opinionated helpers for the WT32-ETH01 happy path.
- **smoltcp**: implements `smoltcp::phy::Device`.
- **embassy-net**: implements `embassy-net-driver`.

The core driver remains usable without any stack or runtime.

---

## Board Support

`boards::wt32_eth01` defines the canonical board configuration:

- Known PHY address and clock requirements
- Convenience helpers for MAC/PHY bring-up

Board helpers are public but remain experimental until more boards are added.

---

## Feature Flags

- `esp32` (default): ESP32 target
- `esp32p4`: placeholder only (not supported)
- `critical-section`: ISR-safe shared access wrappers
- `async`: async wakers and async TX/RX
- `esp-hal`: esp-hal facades
- `smoltcp`: smoltcp integration
- `embassy-net`: embassy-net-driver integration
- `defmt` / `log`: optional logging backends
