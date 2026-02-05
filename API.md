# Public API Inventory and Review

This document inventories the public API surface of `ph-esp32-mac`, recommends stability classifications, and evaluates placement/ergonomics against embedded-Rust conventions.

Last updated: 2026-02-05

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
- `driver` (Stable) – core EMAC types and behavior.
- `hal` (Stable/Advanced) – clock/reset/MDIO helpers for bare-metal bring-up.
- `phy` (Stable) – PHY trait + LAN8720A driver.
- `boards` (Experimental/Board-specific) – opinionated board helpers (ESP32-only).
- `integration` (Provisional) – smoltcp/embassy-net/esp-hal integration.
- `sync` (Stable) – critical-section wrappers + async primitives.
- `constants` (Stable) – common driver constants.
- `unsafe_registers` (Advanced/Unsafe) – raw register accessors.

**Top-level re-exports**
- Core types: `Emac`, `EmacConfig`, `Speed`, `Duplex`, `PhyInterface`, `RmiiClockMode`, checksum/flow control configs. **Stable**. Placement: **good** (central facade).
- Errors: `Error`, `ConfigError`, `DmaError`, `IoError`, plus `Result` aliases. **Stable**. Placement: **good**.
- `InterruptStatus`. **Stable**. Placement: **good**.
- PHY: `Lan8720a`, `Lan8720aWithReset`, `LinkStatus`, `PhyCapabilities`, `PhyDriver`. **Stable**. Placement: **good**.
- Sync (feature `critical-section`): `SharedEmac*` wrappers. **Stable**. Placement: **good**.
- Async (feature `async`): `AsyncEmacState`, `AsyncEmacExt`, `async_interrupt_handler`. **Stable/Provisional**. Placement: **good**.
- Embassy (feature `embassy-net`): `EmbassyEmac`, `EmbassyEmacState`, tokens. **Provisional**. Placement: **ok**, but tokens are implementation details (see notes below).

**Macros**
- `emac_static_sync!` (**Stable**, `critical-section`): Declare `SharedEmac` static in `.dram1` for ESP32.
- `emac_static_async!` (**Stable**, `async`): Declare `StaticCell<Emac<..>>` + `AsyncEmacState`.
- `embassy_net_statics!`, `embassy_net_driver!`, `embassy_net_stack!` (**Provisional**, `embassy-net`): Reduce embassy-net boilerplate.
- `emac_isr!`, `emac_async_isr!` (**Stable**, `esp-hal`): esp-hal ISR helpers.

**Embedded-Rust compliance**
- `#![no_std]` / no allocation: **meets** conventions.
- Explicit error types: **meets** conventions.
- Unsafe isolated behind safe APIs and `unsafe_registers`: **meets** conventions.
- Feature-gated integrations with `doc(cfg)`: **meets** conventions.

---

## Detailed Inventory by Module

### `driver::config` (Stable)

**Items**
- Enums: `Speed`, `Duplex`, `PhyInterface`, `RmiiClockMode`, `DmaBurstLen`, `MacFilterType`, `PauseLowThreshold`, `TxChecksumMode`, `State`.
- Structs: `MacAddressFilter`, `ChecksumConfig`, `FlowControlConfig`, `EmacConfig`.
- Consts: `MAC_FILTER_SLOTS`.

**Placement + notes**
- **Good**: configuration types belong here.
- Builder pattern (`EmacConfig::with_*`) is idiomatic for embedded Rust.
- `EmacConfig::rmii_esp32_default()` is fine; the WT32-ETH01 helper is now documented as the canonical esp-hal path.


### `driver::error` (Stable)

**Items**
- Enums: `ConfigError`, `DmaError`, `IoError`, `Error`.
- Result aliases: `Result`, `ConfigResult`, `DmaResult`, `IoResult`.

**Placement + notes**
- **Good**: centralized, explicit error types match embedded conventions.

### `driver::emac` (Stable)

**Types**
- `Emac<const RX, const TX, const BUF>`
- Aliases: `EmacDefault`, `EmacSmall`, `EmacLarge`

**Lifecycle & state**
- `new`, `init`, `start`, `stop`, `state`

**I/O + buffers**
- `transmit`, `receive`, `rx_available`, `tx_ready`, `can_transmit`, `peek_rx_length`, `tx_descriptors_available`, `rx_frames_waiting`

**Link + MAC config**
- `mac_address`, `set_mac_address`, `speed`, `set_speed`, `duplex`, `set_duplex`, `update_link`
- `set_promiscuous`, `set_pass_all_multicast`, `set_broadcast_enabled`

**MDIO passthrough**
- `read_phy_reg`, `write_phy_reg`

**Interrupts**
- `interrupt_status`, `clear_interrupts`, `clear_all_interrupts`, `handle_interrupt`, `enable_tx_interrupt`, `enable_rx_interrupt`

**Placement + notes**
- **Good** for most items.
- **Advanced**: `read_phy_reg` / `write_phy_reg` overlap with `hal::mdio::MdioController` and are documented as low-level convenience for bare-metal users.

### `driver::filtering` (Stable/Advanced)

**Emac extension methods**
- Perfect filter: `add_mac_filter`, `add_mac_filter_config`, `remove_mac_filter`, `clear_mac_filters`, `mac_filter_count`, `has_free_mac_filter_slot`
- Hash filter: `add_hash_filter`, `remove_hash_filter`, `check_hash_filter`, `clear_hash_table`, `hash_table`, `set_hash_table`, `enable_hash_multicast`, `enable_hash_unicast`, `compute_hash_index`
- VLAN: `set_vlan_filter`, `configure_vlan_filter`, `disable_vlan_filter`, `is_vlan_filter_enabled`, `vlan_filter_id`

**Placement + notes**
- **Good**: these are logically part of `Emac` behavior.
- **Advanced**: Hash/VLAN filters are niche; documented as advanced with limited hardware validation notes.

### `driver::flow` (Stable/Advanced)

**Emac extension methods**
- `enable_flow_control`, `set_peer_pause_ability`, `check_flow_control`, `is_flow_control_active`, `flow_control_config`, `peer_pause_ability`

**Placement + notes**
- **Good**: logically belongs on `Emac`.
- **Advanced**: flow control is niche; documented as advanced with limited hardware validation notes.

### `driver::interrupt` (Stable)

**Items**
- `InterruptStatus` struct + helpers (`from_raw`, `to_raw`, `any`, `has_error`).

**Placement + notes**
- **Good**: clean and idiomatic; field names are clear.

---

### `hal::clock` (Advanced)

**Items**
- `ClockState`, `ClockController`.

**Placement + notes**
- **Good** for bare-metal users.
- For esp-hal users, the facade already hides these details.

### `hal::mdio` (Stable)

**Items**
- `MdcClockDivider`, `MdioBus`, `MdioController`.
- `PhyStatus`.
- Helpers: `read_phy_status`, `reset_phy`, `read_phy_id`, `enable_auto_negotiation`, `force_speed_duplex`.

**Placement + notes**
- **Good** and idiomatic; the `MdioBus` trait enables testability.

### `hal::reset` (Advanced)

**Items**
- `ResetController`, `ResetState`, `ResetManager`, `full_reset`.

**Placement + notes**
- **Good** for bare-metal users; advanced usage.

---

### `phy::generic` (Stable)

**Items**
- `LinkStatus`, `PhyCapabilities`, `PhyDriver`.

**Placement + notes**
- **Good**: clear, minimal trait and capability struct.

### `phy::lan8720a` (Stable)

**Items**
- `Lan8720a`, `Lan8720aWithReset`.
- `LAN8720A_PHY_ID`, `LAN8720A_PHY_ID_MASK`.
- Helpers: `wait_for_link`, `scan_bus`.

**Placement + notes**
- **Good**: LAN8720A is the canonical PHY for ESP32 Ethernet.
- The helpers are useful; keep **Stable**.

**Hidden**
- IEEE 802.3 register constants are `doc(hidden)`; treated as **Internal/Hidden**.

---

### `boards::wt32_eth01` (Experimental/Board-specific)

**Items**
- `Wt32Eth01` constants + helpers (`emac_config`, `emac_config_with_mac`, `lan8720a`, `is_valid_phy_id`, etc.).

**Placement + notes**
- **Good**: explicit board module reduces boilerplate and matches the canonical example.
- Board helpers should remain **Experimental** until a second board is added or the API stabilizes.

---

### `integration::esp_hal` (Stable, feature `esp-hal`)

**Items**
- `EmacBuilder` (plus WT32 constructors), `EmacPhyBundle`, `EmacExt`.
- `EMAC_INTERRUPT`, `emac_isr!`, `emac_async_isr!`.
- Re-exports: `Delay`, `Interrupt`, `Priority`, `InterruptHandler`.
- `Wt32Eth01` (ESP32-only board helper).

**Placement + notes**
- **Good**: this is the primary public facade for esp-hal users.
- The WT32-specific helpers are correctly scoped here to lower cognitive burden.

### `integration::embassy_net` (Provisional, feature `embassy-net`)

**Items**
- `EmbassyEmacState`, `EmbassyEmac`, `EmbassyRxToken`, `EmbassyTxToken`.

**Placement + notes**
- **Ok**, but token types are documented as “not intended for direct use.”
- Macro helpers (`embassy_net_*`) reduce boilerplate for esp-hal runtime usage.

### `integration::smoltcp` (Provisional, feature `smoltcp`)

**Items**
- `EmacRxToken`, `EmacTxToken`, `ethernet_address` helper.

**Placement + notes**
- **Ok**, but tokens are low-level and documented as **Advanced** implementation details.

---

### `sync` (Stable, feature `critical-section` / `async`)

**Items**
- Wrappers: `SharedEmac*` and `AsyncSharedEmac*` type aliases (**Stable**).
- Async: `AsyncEmacState`, `AsyncEmacExt`, `RxFuture`, `TxFuture`, `ErrorFuture`, `async_interrupt_handler`, `peek_interrupt_status`, `reset_async_state` (**Stable/Provisional**).

**Placement + notes**
- **Good**: wrappers are idiomatic for ISR-safe access.
- `CriticalSectionCell`/`AtomicWaker` are internal (not part of the public API).
---

### `constants` (Stable)

**Items**
- Buffer sizes, MTU, timing constants, flow defaults, etc.

**Placement + notes**
- **Good**: avoids cluttering root namespace.

### `unsafe_registers` (Advanced/Unsafe)

**Items**
- `DmaRegs`, `MacRegs`, `ExtRegs`.

**Placement + notes**
- **Good**: explicit unsafe boundary. Keep strongly documented as advanced. *** agree***

---

## Recommendations (for your review)

1. **Mark MDIO passthrough on `Emac` as Advanced** in docs; keep for convenience but discourage for most users (use `MdioController` instead).
2. **Remove `hal::gpio::esp32_gpio`** entirely (first release allows breaking changes); steer users to `boards::wt32_eth01`.
3. **Token types** in smoltcp/embassy-net are documented as implementation details to reduce cognitive load.
4. **Keep esp-hal facade primary**: recommend esp-hal users start from `EmacBuilder::wt32_eth01_*` and `EmacPhyBundle::wt32_eth01_lan8720a`.

Once you confirm the classifications, I can implement any API surface adjustments.
