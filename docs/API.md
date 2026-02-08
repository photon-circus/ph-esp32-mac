# Public API Inventory

This document inventories the public API surface of `ph-esp32-mac`, recommends
stability classifications, and evaluates placement/ergonomics against embedded
Rust conventions. It is a transitional planning document for the 0.1.0 release
and is not expected to be maintained long-term. Last updated: 2026-02-05.

---

## Table of Contents

- [Status](#status)
- [Legend](#legend)
- [Top-Level Facade (crate root)](#top-level-facade-crate-root)
- [Detailed Inventory by Module](#detailed-inventory-by-module)
- [Recommendations](#recommendations)

---

## Status

This file is used to review API shape and stability before the initial release.
Once 0.1.0 is published, the document may be archived or removed.

---

## Legend

- **Stable**: Intended for long-term use; breaking changes avoided.
- **Provisional**: Public but may evolve based on ecosystem feedback.
- **Advanced**: Expert-level or low-level usage; more footguns.
- **Experimental**: Feature-gated or board-specific; may change.
- **Internal/Hidden**: `doc(hidden)` or unsafe-register access.

---

## Top-Level Facade (crate root)

**Public modules**
- `driver` (Stable) - core EMAC types and behavior.
- `hal` (Stable/Advanced) - clock/reset/MDIO helpers for bare-metal bring-up.
- `phy` (Stable) - PHY trait + LAN8720A driver.
- `boards` (Experimental/Board-specific) - opinionated board helpers (ESP32-only).
- `integration` (Provisional) - smoltcp/embassy-net/esp-hal integration.
- `sync` (Stable) - critical-section wrappers + async primitives.
- `constants` (Stable) - common driver constants.
- `unsafe_registers` (Advanced/Unsafe) - raw register accessors.

**Top-level re-exports**
- Core types: `Emac`, `EmacConfig`, `Speed`, `Duplex`, `PhyInterface`,
  `RmiiClockMode`, checksum/flow control configs. **Stable**. Placement: good.
- Errors: `Error`, `ConfigError`, `DmaError`, `IoError`, plus `Result` aliases.
  **Stable**. Placement: good.
- `InterruptStatus`. **Stable**. Placement: good.
- PHY: `Lan8720a`, `Lan8720aWithReset`, `LinkStatus`, `PhyCapabilities`,
  `PhyDriver`. **Stable**. Placement: good.
- Sync (feature `critical-section`): `SharedEmac*` wrappers. **Stable**.
- Async (feature `async`): `AsyncEmacState`, `AsyncEmacExt`,
  `async_interrupt_handler`. **Stable/Provisional**.
- Embassy (feature `embassy-net`): `EmbassyEmac`, `EmbassyEmacState`, tokens.
  **Provisional**.

**Macros**
- `emac_static_sync!` (**Stable**, `critical-section`): Declare `SharedEmac`
  static in `.dram1` for ESP32.
- `emac_static_async!` (**Stable**, `async`): Declare `StaticCell<Emac<..>>` +
  `AsyncEmacState`.
- `embassy_net_statics!`, `embassy_net_driver!`, `embassy_net_stack!`
  (**Provisional**, `embassy-net`): Reduce embassy-net boilerplate.
- `emac_isr!`, `emac_async_isr!` (**Stable**, `esp-hal`): esp-hal ISR helpers.

---

## Detailed Inventory by Module

### `driver::config` (Stable)

**Items**
- Enums: `Speed`, `Duplex`, `PhyInterface`, `RmiiClockMode`, `DmaBurstLen`,
  `MacFilterType`, `PauseLowThreshold`, `TxChecksumMode`, `State`.
- Structs: `MacAddressFilter`, `ChecksumConfig`, `FlowControlConfig`, `EmacConfig`.
- Consts: `MAC_FILTER_SLOTS`.

**Notes**
- Builder pattern (`EmacConfig::with_*`) is idiomatic for embedded Rust.
- `EmacConfig::rmii_esp32_default()` encodes ESP32 defaults.

---

### `driver::error` (Stable)

**Items**
- Enums: `ConfigError`, `DmaError`, `IoError`, `Error`.
- Result aliases: `Result`, `ConfigResult`, `DmaResult`, `IoResult`.

**Notes**
- Centralized, explicit error types match embedded conventions.

---

### `driver::emac` (Stable)

**Types**
- `Emac<const RX, const TX, const BUF>`
- Aliases: `EmacDefault`, `EmacSmall`, `EmacLarge`

**Lifecycle**
- `new`, `init`, `start`, `stop`, `state`

**I/O + buffers**
- `transmit`, `receive`, `rx_available`, `tx_ready`, `can_transmit`,
  `peek_rx_length`, `tx_descriptors_available`, `rx_frames_waiting`

**Link + MAC config**
- `mac_address`, `set_mac_address`, `speed`, `set_speed`, `duplex`, `set_duplex`,
  `update_link`
- `set_promiscuous`, `set_pass_all_multicast`, `set_broadcast_enabled`

**MDIO passthrough**
- `read_phy_reg`, `write_phy_reg`

**Interrupts**
- `interrupt_status`, `clear_interrupts`, `clear_all_interrupts`,
  `handle_interrupt`, `enable_tx_interrupt`, `enable_rx_interrupt`

**Notes**
- `read_phy_reg` / `write_phy_reg` are advanced convenience helpers.

---

### `driver::filtering` (Stable/Advanced)

**Emac extension methods**
- Perfect filter: `add_mac_filter`, `add_mac_filter_config`,
  `remove_mac_filter`, `clear_mac_filters`, `mac_filter_count`,
  `has_free_mac_filter_slot`
- Hash filter: `add_hash_filter`, `remove_hash_filter`, `check_hash_filter`,
  `clear_hash_table`, `hash_table`, `set_hash_table`, `enable_hash_multicast`,
  `enable_hash_unicast`, `compute_hash_index`
- VLAN: `set_vlan_filter`, `configure_vlan_filter`, `disable_vlan_filter`,
  `is_vlan_filter_enabled`, `vlan_filter_id`

**Notes**
- Hash and VLAN filters are advanced; hardware validation is limited.

---

### `driver::flow` (Stable/Advanced)

**Emac extension methods**
- `enable_flow_control`, `set_peer_pause_ability`, `check_flow_control`,
  `is_flow_control_active`, `flow_control_config`, `peer_pause_ability`

**Notes**
- Flow control is niche; treat as advanced.

---

### `driver::interrupt` (Stable)

**Items**
- `InterruptStatus` struct + helpers (`from_raw`, `to_raw`, `any`, `has_error`).

---

### `hal::clock` (Advanced)

**Items**
- `ClockState`, `ClockController`.

**Notes**
- Intended for bare-metal bring-up; esp-hal users should prefer the facade.

---

### `hal::mdio` (Stable)

**Items**
- `MdcClockDivider`, `MdioBus`, `MdioController`, `PhyStatus`.
- Helpers: `read_phy_status`, `reset_phy`, `read_phy_id`,
  `enable_auto_negotiation`, `force_speed_duplex`.

---

### `hal::reset` (Advanced)

**Items**
- `ResetController`, `ResetState`, `ResetManager`, `full_reset`.

---

### `phy::generic` (Stable)

**Items**
- `LinkStatus`, `PhyCapabilities`, `PhyDriver`.

---

### `phy::lan8720a` (Stable)

**Items**
- `Lan8720a`, `Lan8720aWithReset`.
- `LAN8720A_PHY_ID`, `LAN8720A_PHY_ID_MASK`.
- Helpers: `wait_for_link`, `scan_bus`.

**Hidden**
- IEEE 802.3 register constants are `doc(hidden)`.

---

### `boards::wt32_eth01` (Experimental/Board-specific)

**Items**
- `Wt32Eth01` constants + helpers (`emac_config`, `emac_config_with_mac`,
  `lan8720a`, `is_valid_phy_id`, etc.).

**Notes**
- This is the canonical board path for esp-hal users.

---

### `integration::esp_hal` (Stable, feature `esp-hal`)

**Items**
- `EmacBuilder`, `EmacPhyBundle`, `EmacExt`.
- `EMAC_INTERRUPT`, `emac_isr!`, `emac_async_isr!`.
- Re-exports: `Delay`, `Interrupt`, `Priority`, `InterruptHandler`.
- `Wt32Eth01` helper re-export.

---

### `integration::embassy_net` (Provisional, feature `embassy-net`)

**Items**
- `EmbassyEmacState`, `EmbassyEmac`, `EmbassyRxToken`, `EmbassyTxToken`.

**Notes**
- Token types are implementation details; avoid direct use.

---

### `integration::smoltcp` (Provisional, feature `smoltcp`)

**Items**
- `EmacRxToken`, `EmacTxToken`, `ethernet_address` helper.

**Notes**
- Token types are low-level; avoid direct use where possible.

---

### `sync` (Stable, features `critical-section` / `async`)

**Items**
- Wrappers: `SharedEmac*` and `AsyncSharedEmac*` type aliases.
- Async: `AsyncEmacState`, `AsyncEmacExt`, `RxFuture`, `TxFuture`,
  `ErrorFuture`, `async_interrupt_handler`, `peek_interrupt_status`,
  `reset_async_state`.

---

### `constants` (Stable)

**Items**
- Buffer sizes, MTU, timing constants, flow defaults, etc.

---

### `unsafe_registers` (Advanced/Unsafe)

**Items**
- `DmaRegs`, `MacRegs`, `ExtRegs`.

**Notes**
- Explicit unsafe boundary. Prefer safe APIs where possible.

---

## Recommendations

1. Mark MDIO passthrough on `Emac` as advanced in docs; prefer `MdioController`.
2. Keep the esp-hal facade as the primary "happy path" for users.
3. Treat token types in smoltcp/embassy-net as implementation details.
