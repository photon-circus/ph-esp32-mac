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
- `esp32p4` is advertised but incomplete (placeholder pins in `src/internal/gpio_pins.rs` and partial register differences).
- CI covers clippy/coverage only; no `cargo test --lib`, no `cargo fmt -- --check`, no `cargo doc --no-deps`, and no MSRV enforcement.
- Packaging is not curated; older/internal docs and local artifacts are likely to be included unless excluded.

---

## Phase 0: Scope

- Decide publish target (crates.io vs internal) and explicitly declare supported hardware scope (ESP32-only vs ESP32+P4).
- Declare primary consumers: `esp-hal` and `embassy-net`, with async support as a first-class goal.
- Lock the MSRV policy and make `rust-version` in `Cargo.toml` match CI’s toolchain.

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

- Provide a first-class esp-hal constructor or builder that accepts:
  - `esp_hal::delay::Delay` (or `DelayNs`) for timing.
  - A consistent way to enable EMAC interrupts via `EmacExt` and helpers.
- Consolidate pin/clock configuration docs in a single, esp-hal-oriented section.
- Ensure the existing `EmacExt` API matches esp-hal 1.0.0 interrupt expectations and update examples accordingly.

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

- Replace global wakers with per-instance state where practical to avoid cross-instance interference.
- Use interrupt-driven wakeups for RX/TX/link to avoid busy-loop polling.
- Keep all async paths `no_std`/`no_alloc`, relying on static buffers and existing DMA rings.

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
- Either implement ESP32-P4 fully (pins, register map, descriptor alignment, tests) or mark it “experimental/unsupported” and hide from docs.
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
