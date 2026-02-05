# Publishability Analysis and Roadmap

This document captures the current publishability analysis of `ph-esp32-mac` and a phased roadmap to make it ready for release. It is intended for follow-up planning and tracking.

---

## Table of Contents

1. [Snapshot](#snapshot)
2. [Phase 0: Scope](#phase-0-scope)
3. [Phase 1: Packaging](#phase-1-packaging)
4. [Phase 2: Documentation and Examples](#phase-2-documentation-and-examples)
5. [Phase 3: esp-hal and Embassy Async Integration](#phase-3-esp-hal-and-embassy-async-integration)
6. [Phase 4: API and Feature Hygiene](#phase-4-api-and-feature-hygiene)
7. [Phase 5: CI and Verification](#phase-5-ci-and-verification)
8. [Phase 6: Release](#phase-6-release)
9. [Publishable Definition](#publishable-definition)

---

## Snapshot

- Missing publish metadata and entry docs: no root `README.md`, no `CHANGELOG.md`, and `Cargo.toml` lacks `repository`, `documentation`, `homepage`, and `readme` fields.
- Doc inconsistencies: multiple public docs still reference the old crate name `esp32_emac` in `src/lib.rs`, `src/phy/mod.rs`, `src/phy/lan8720a.rs`, and `src/integration/smoltcp.rs`.
- Integration docs are stale: `integration_tests/README.md` references `ph_esp32_mac::smoltcp::EmacDevice`, which does not exist (the `Device` impl is on `Emac`).
- ESP32-P4 is out of scope for this release; keep it experimental/hidden and avoid advertising it in user-facing docs.
- CI covers clippy/coverage only; no `cargo test --lib`, no `cargo fmt -- --check`, no `cargo doc --no-deps`, and no MSRV enforcement.
- Packaging is not curated; older/internal docs and local artifacts are likely to be included unless excluded.

---

## Phase 0: Scope

- Publish target: crates.io. Explicitly declare supported hardware scope (ESP32-only for the next release).
- Primary consumer: `esp-hal` (with `embassy-net` support as a secondary integration).
- MSRV is locked to Rust 1.92.0; keep `rust-version` aligned with CI’s toolchain.

---

## Phase 1: Packaging

- Add a root `README.md` that follows `DOCUMENTATION_STANDARDS.md`, then set `readme = "README.md"` in `Cargo.toml`.
- Add `repository`, `documentation`, and `homepage` fields to `Cargo.toml`; add `package.metadata.docs.rs` with a safe feature set for docs.rs.
- Add an `exclude` list in `Cargo.toml` for non-package artifacts (`target/`, `.idea/`, `DESIGN_old.md`, `REORGANIZATION.md`, `CLAUDE.md`, `integration_tests/target/`).
- Add `CHANGELOG.md` and a release checklist file (e.g., `RELEASE.md`).

---

## Phase 2: Documentation and Examples

- Replace all `esp32_emac` references with `ph_esp32_mac` in public docs: `src/lib.rs`, `src/phy/mod.rs`, `src/phy/lan8720a.rs`, `src/integration/smoltcp.rs`.
- Update `integration_tests/README.md` to match the current smoltcp integration (use `Emac`’s `Device` impl).
- Reconcile `DESIGN.md` and `TESTING.md` with the current module layout (`src/driver/...`) and actual test counts; add “as of” dates and regeneration commands.
- Bring Markdown files into compliance with `DOCUMENTATION_STANDARDS.md` (intro paragraph, `---`, TOC, language-tagged code fences).

---

## Phase 3: esp-hal and Embassy Async Integration

### 3.1 Driver Model and Dependencies

- Use `embassy-net-driver` directly (not `embassy-net`) for the driver trait, per Embassy guidance.
- Confirm the `embassy-net` 0.7.0 dependency graph and pin the driver crate version used by that release.
- Consider an optional `embassy-net-driver-channel` path for a background-task “driver loop” (tradeoffs: easier polling vs extra task/queues).

### 3.2 esp-hal API Surface (target: `esp-hal` **1.0.0**)

- **Goal:** Make `esp-hal` the primary, first-class facade so users can bring up EMAC with minimal boilerplate while preserving the driver’s stronger internal implementation.
- **Breaking changes are acceptable** if they reduce setup friction for esp-hal consumers.

**Facade & Constructor Strategy**
- Provide a top-level esp-hal constructor (or builder) that hides raw register setup and reduces the “steps list” to a single call:
  - `Emac::new_esp_hal(peripherals, config, clocks, dma, ...)` or `EmacBuilder::new(peripherals).with_clocks(...).with_phy(...).build()`.
  - Accept `esp_hal::delay::Delay` (or `DelayNs`) directly.
  - Accept `esp_hal` peripheral ownership patterns rather than raw `*mut` or statics wherever possible.
- Add a minimal “happy path” facade that defaults the common RMII settings:
  - `EmacConfig::esp_hal_default()` or `EmacConfig::rmii_esp32_default()`.
  - Default RMII clock mode and GPIO routing for ESP32 (documented pins only).

**Proposed API Checklist (esp-hal-first facade)**
- [x] `EmacBuilder::new(&mut Emac)` with esp-hal defaults
- [x] `EmacConfig::rmii_esp32_default()` (or `EmacConfig::esp_hal_default()`)
- [x] `EmacExt::bind_interrupt(handler)` (or `Emac::bind_interrupt(handler)`)
- [x] `Emac::handle_interrupt()` (clears status + wakes async state if present)
- [ ] `EmacPhyBundle::new(emac, phy, mdio, delay)` (optional convenience)
- [ ] `EmacPhyBundle::wait_link_up(timeout)` helper
- [ ] `doc(cfg(feature = "esp-hal"))` and “happy path” snippet

**Interrupt Wiring & Ergonomics**
- Provide a single helper to bind interrupts and wire async wakeups:
  - Example: `emac.enable_interrupts(handler)` or `EmacExt::bind_interrupt(handler)`; align with esp-hal 1.0.0 expectations.
  - Expose a `handle_interrupt()` method on the esp-hal facade to avoid manual status read/clear boilerplate.
- Provide an opt-in `esp_hal`-specific “interrupt glue” module that:
  - Exposes `emac_isr!` helpers or a ready-to-use handler stub.
  - Bridges to `AsyncEmacState` so async users don’t manually stitch ISR logic.

**PHY + Link Management Simplification**
- Offer a thin esp-hal wrapper that bundles PHY + MDIO setup:
  - `EmacWithPhy::new(emac, phy, mdio, delay)` or `EmacPhyBundle`.
  - Provide a convenience `wait_link_up()` helper that hides repeated polling loops.
- Document the recommended PHY reset path for esp-hal (optional pin), with a single helper call.

**Boilerplate Reduction Targets**
- Reduce bring-up to ~10 lines in esp-hal examples:
  - One call for clocks/pins, one for EMAC init/start, one for PHY init/link, one for interrupt binding.
- Provide one minimal esp-hal example with just link-up + DHCP/stack (no extra logging).
- Provide one advanced esp-hal example showing async integration with `AsyncEmacState` + ISR binding.

**Before / After (Goal)**

_Before (current, verbose)_

```rust
// Pseudocode: multiple steps + manual wiring
let mut delay = Delay::new();
let mut emac = unsafe { &mut EMAC };
emac.init(config, &mut delay)?;
emac.start()?;

let mut mdio = MdioController::new(delay);
let mut phy = Lan8720a::new(0);
phy.init(&mut mdio)?;

emac.bind_interrupt(handler);
```

_After (esp-hal facade, minimal boilerplate)_

```rust
let mut delay = Delay::new();
let (emac, mut phy) = EmacBuilder::new(peripherals)
    .with_config(EmacConfig::rmii_esp32_default())
    .with_delay(&mut delay)
    .build()?;

emac.bind_interrupt(handler);
emac.start()?;
phy.wait_link_up(&mut emac, &mut delay)?;
```

_Async + ISR wiring (esp-hal facade)_

```rust
static ASYNC_STATE: AsyncEmacState = AsyncEmacState::new();

emac_isr!(EMAC_ISR, Priority::Priority1, {
    ph_esp32_mac::async_interrupt_handler(&ASYNC_STATE);
});

emac.bind_interrupt(EMAC_ISR);
```

**Implementation Sprints (Concrete)**

**Sprint 1 — Facade Foundations (1 week)**
- **Status:** ✅ Completed
- **Goals:** Define the esp-hal-first API shape and land the core builder + defaults.
- **Work items:**
  - Add `EmacBuilder::new(&mut Emac)` with minimal required params.
  - Add `EmacConfig::rmii_esp32_default()`.
  - Ensure builder wires clock/pin defaults for ESP32 RMII.
  - Add `doc(cfg(feature = "esp-hal"))` on new APIs.
  - Update one esp-hal example to use the new builder (compile check).
- **Deliverables:** New facade types compile, basic esp-hal example updated.
- **Exit criteria:** Host `cargo check --features esp-hal` requires the esp toolchain (esp-rom-sys target features); examples compile on target toolchain.

**Sprint 2 — Interrupt Wiring (1 week)**
- **Status:** ✅ Completed
- **Goals:** Reduce ISR wiring boilerplate for esp-hal users.
- **Work items:**
  - Add `Emac::handle_interrupt()` and `EmacExt::bind_interrupt(handler)` (or equivalent).
  - Add esp-hal glue (`emac_isr!` helper or stub) for a ready-to-use handler.
  - Document ISR wiring in a single “happy path” section.
- **Deliverables:** ISR wiring helper(s) and esp-hal example updated to use them.
- **Exit criteria:** esp-hal example builds with no manual status read/clear.

**Sprint 3 — Async Integration (1 week)**
- **Status:** ✅ Completed
- **Goals:** Integrate per-instance async state into esp-hal flows.
- **Work items:**
  - Ensure async path uses `AsyncEmacState` in esp-hal example(s).
  - Wire async ISR glue to call `async_interrupt_handler(&AsyncEmacState)`.
  - Add async-specific docs section with the exact ISR + task pattern.
- **Deliverables:** Async example added; async ISR macro references `AsyncEmacState`.
- **Exit criteria:** esp-hal + async example builds; async ISR wiring is one-line.

**Sprint 4 — PHY/Link Convenience & Polishing (1 week)**
- **Goals:** Minimize bring-up steps and consolidate PHY handling.
- **Work items:**
  - Add `EmacPhyBundle` (or `EmacWithPhy`) convenience wrapper.
  - Add `wait_link_up()` helper with timeout/backoff.
  - Update examples to use bundle + link helper.
  - Add concise “bring-up in ~10 lines” snippet in docs.
- **Deliverables:** PHY/link helper API + updated docs/examples.
- **Exit criteria:** esp-hal example is < ~10 lines of setup; docs show minimal flow.

**API Alignment + Docs**
- Consolidate esp-hal specific docs into one section with a single “happy path” snippet.
- Ensure `EmacExt` matches esp-hal 1.0.0 API names, interrupt model, and ownership patterns.
- Add `doc(cfg(feature = "esp-hal"))` for esp-hal-specific APIs and examples.

### 3.3 Embassy Driver Integration (target: `embassy-net` **0.7.0**)

- Add a new module behind a feature flag (e.g., `embassy-net`) that exposes a dedicated driver wrapper:
  - `EmbassyEmac<'a, RX, TX, BUF>` (or similar) implementing `embassy_net_driver::Driver`.
  - `EmbassyRxToken` and `EmbassyTxToken` implementing the token traits.
- Map the driver trait requirements to EMAC behavior:
  - `receive()` returns RX+TX tokens when a frame is ready; otherwise registers the waker.
  - `transmit()` returns a TX token when space exists; otherwise registers the waker.
  - `link_state()` returns cached link status and registers a waker for link changes.
- Provide a link-state update path:
  - Poll PHY status periodically or from an interrupt path.
  - Update cached state and wake the link waker on transitions.

### 3.4 Async/Waker Architecture

- **Status:** ✅ Implemented (breaking change accepted).
- **Implemented architecture:**
  - `AsyncEmacState` provides per-instance RX/TX/ERR wakers with `const fn new()`.
  - `AsyncEmacExt::{receive_async, transmit_async, wait_for_error}` now require `&AsyncEmacState`.
  - `async_interrupt_handler(state)` and `reset_async_state(state)` are per-instance helpers.
  - `AsyncSharedEmac` owns an async state and exposes `handle_interrupt()` for ISR wiring.
  - Async futures use the “register → recheck → Pending” pattern to avoid missed wakeups.
- **Constraints preserved:** `no_std`/`no_alloc`, static buffers, existing DMA rings.
- **Validation:**
  - Unit tests cover per-instance isolation and reset behavior.
  - Docs updated with the new ISR call pattern and esp-hal binding example.

### 3.5 Embassy Examples and Runner Model

- Provide a full `embassy-net` example using:
  - `embassy_net::new()` to create `Stack` + `Runner`.
  - `Runner::run()` in a background task.
  - Static resources (buffers, stack storage, device instance).
- Add a minimal esp-hal + embassy example that shows:
  - Interrupt binding.
  - Driver creation.
  - Network stack bring-up (static IP and/or DHCP).

### 3.6 Acceptance Criteria

- `embassy-net` example builds with `embassy-net` 0.7.0 and `esp-hal` 1.0.0 toolchains.
- RX/TX tasks do not busy-loop when idle (verified by waker usage).
- Link changes wake the stack and update `link_state()` correctly.

---

## Phase 4: API and Feature Hygiene

- Enforce feature exclusivity (`esp32` vs `esp32p4`) with `compile_error!` and document the behavior.
- Keep ESP32-P4 as experimental/unsupported and hidden from docs until fully implemented.
- Perform an unsafe-audit pass to ensure every `unsafe` block has a `SAFETY:` comment and every unsafe public API has a `# Safety` section.
- Add `cfg_attr(docsrs, doc(cfg(...)))` for feature-gated APIs to improve docs.rs clarity.

---

## Phase 5: CI and Verification

- Add CI jobs for `cargo test --lib`, `cargo fmt -- --check`, `cargo doc --no-deps`, and an MSRV job tied to `rust-version`.
- Add a feature matrix `cargo check` (default, `smoltcp`, `async`, `critical-section`) and explicitly document `esp-hal` build expectations (either target CI or manual steps).
- Add `cargo publish --dry-run` (or `cargo package --list`) to catch packaging warnings early.

---

## Phase 6: Release

- Establish release versioning rules (0.x compatibility expectations) and update `CHANGELOG.md`.
- Run `cargo publish --dry-run`, then publish and tag; update README badges and docs.rs links.

---

## Publishable Definition

- `cargo publish --dry-run` passes with no warnings.
- `cargo test --lib`, `cargo clippy --lib --tests -- -D warnings`, and `cargo doc --no-deps` all pass in CI.
- README/CHANGELOG present; docs/examples show the correct crate name and current API.
- Supported hardware/features are explicit in README and docs.
- `esp-hal` and `embassy-net` integration examples compile for the target toolchain.
