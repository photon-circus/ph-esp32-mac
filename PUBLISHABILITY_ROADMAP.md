# Publishability Analysis and Roadmap

This document tracks the current publishability status of `ph-esp32-mac` and a sprint plan to reach a crates.io-ready release. It prioritizes API correctness, embedded-Rust idioms, and minimal user boilerplate.

Last updated: 2026-02-05

---

## Table of Contents

1. [Snapshot](#snapshot)
2. [Current Gaps](#current-gaps)
3. [Scope and Targets](#scope-and-targets)
4. [Sprint Plan](#sprint-plan)
5. [Publishable Definition](#publishable-definition)

---

## Snapshot

- Examples are working: `smoltcp_echo`, `esp_hal_integration`, `esp_hal_async`, and `embassy_net` now run as expected.
- Integration tests run as expected (per latest local runs).
- Async/waker architecture is per-instance and no-alloc, using `AsyncEmacState` and explicit ISR helpers.
- esp-hal (1.0.0) and embassy-net (0.7.0) integrations are present and exercised via examples.
- Packaging baseline is in place: root `README.md`, `CHANGELOG.md`, `RELEASE.md`, docs.rs metadata, and publish metadata are set.
- Examples follow Rust conventions but are excluded from the crates.io package due to cross-compile complexity; README links to the repo examples.
- `cargo publish --dry-run` succeeds locally (one Windows incremental cache warning observed).

---

## Current Gaps

- **Examples packaging decision**: Current choice is to exclude examples from the published crate; consider a separate `ph-esp32-mac-examples` crate or keep repo-only.
- **CI discipline**: Confirm fmt/doc/clippy are aligned with the packaging choice (examples excluded from publish, still built in repo as needed).
 - **Tooling note**: On Windows, clippy/doc runs may emit an incremental cache access warning (does not affect output).

---

## Scope and Targets

- **Publish target**: crates.io
- **Supported hardware for next release**: ESP32 only
- **Experimental / hidden**: ESP32-P4 (feature hidden from docs; not supported in 0.1)
- **Primary consumer**: `esp-hal` 1.0.0
- **Secondary integration**: `embassy-net` 0.7.0
- **MSRV**: Rust 1.92.0 (locked)

---

## Sprint Plan

### Sprint 0 - Packaging and Release Hygiene

**Goal**: Make the crate publishable from a packaging perspective.

**Status**: ✅ Completed

**Work items**
- Add root `README.md` (aligned with `DOCUMENTATION_STANDARDS.md`) and set `readme = "README.md"`.
- Add `repository`, `documentation`, `homepage`, and `package.metadata.docs.rs`.
- Add `CHANGELOG.md` and `RELEASE.md` with a publish checklist.
- Add `exclude` list to `Cargo.toml` for non-package artifacts.
- Disable auto-discovered examples (`autoexamples = false`) and link to repo examples instead.

**Exit criteria**
- `cargo package --list` is clean and includes only intended artifacts.
- `cargo publish --dry-run` passes locally.

---

### Sprint 1 - Public API Audit and Embedded-Rust Idioms

**Goal**: Ensure the public surface is idiomatic, minimal, and stable for embedded users (esp-hal first).

**Status**: ✅ Completed

**Work items**
- Inventory all public items across `driver`, `integration`, and `sync` modules; categorize as stable vs internal.
- Tighten re-exports to reduce surface bloat; expose a clean top-level facade.
- Define opinionated facades for esp-hal/embassy-net/smoltcp to reduce boilerplate and guesswork.
- Verify API names and patterns align with embedded-Rust norms:
  - Builder patterns, explicit `Result` types, `no_std` conventions.
  - Minimize unsafe exposure and raw pointers in public APIs.
- Review feature gating and doc(cfg) coverage:
  - `esp-hal`, `embassy-net`, `smoltcp`, `async`, `critical-section`.
- Run an unsafe-audit pass:
  - All `unsafe` blocks have `SAFETY:` comments.
  - All unsafe public APIs have a `# Safety` section.

**Exit criteria**
- A “Public API report” is captured in `API.md` (or embedded here).
- All public APIs follow documented style and safety requirements.
- No clippy warnings for `missing_docs` or unsafe documentation in public items.

**Progress**
- Demoted low-level register re-exports into `unsafe_registers::{DmaRegs, ExtRegs, MacRegs}`.
- Created initial `API.md` inventory.
- Removed top-level HAL re-exports and moved constants under `ph_esp32_mac::constants`.
- Added `doc(cfg)` coverage for feature-gated modules and re-exports.
- Simplified esp-hal facade: removed `EspHalEmac` placeholder and made explicit esp-hal re-exports.
- Marked ESP32-P4 as experimental and hidden from docs (feature is present but not documented).
- Added WT32-ETH01 board helper and esp-hal convenience constructors for the canonical bring-up.
- Completed API inventory and stability classification in `API.md`.
- Removed `hal::gpio::esp32_gpio` (breaking removal allowed for first release).
- Documented advanced/testing gaps for filtering and flow control.
- Documented token types as implementation details for smoltcp/embassy-net.

---

### Sprint 2 - Documentation and Examples Polish

**Goal**: Present a clear, minimal integration story with current examples and docs.

**Work items**
- **Root documentation pass**
  - Add “happy path” snippets for esp-hal (WT32-ETH01), embassy-net, and smoltcp.
  - Add a concise **feature-flag matrix** (what each flag unlocks).
  - Add a **memory/footprint** section (DMA descriptor counts, buffer sizes, defaults).
  - Ensure all docs use the correct crate name and current API (no legacy references).
- **Examples + integration tests docs**
  - Update `examples/README.md` to match current `cargo` aliases and example layout.
  - Update `integration_tests/README.md` with the latest board wiring + run steps.
  - Add a small troubleshooting note for DHCP bring-up timing and link readiness.
- **Packaging narrative**
  - Document the decision to keep examples repo-only (or spin out a separate examples crate).
  - If repo-only, add a short “why” and link from the root README.

**Exit criteria**
- Docs build cleanly with `cargo doc --no-deps`.
- Example and integration-test READMEs are accurate, minimal, and up-to-date.

---

### Sprint 3 - CI and Verification Coverage

**Goal**: Make CI reflect release confidence.

**Work items**
- Ensure CI runs:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --lib`
  - `cargo doc --no-deps`
- Add a feature matrix for host checks (default, `smoltcp`, `async`, `critical-section`).
- Document xtensa target checks (manual or optional CI job depending on toolchain availability).

**Exit criteria**
- CI is green on main with expected feature matrix.
- Manual target checks are documented and reproducible.

---

### Sprint 4 - Release Readiness

**Goal**: Prepare for the first crates.io release.

**Work items**
- Freeze the public API for 0.1.x and document compatibility expectations.
- Run `cargo publish --dry-run` and fix any remaining warnings.
- Tag release and update README badges.

**Exit criteria**
- Release checklist complete.
- Dry-run publish passes.

---

## Publishable Definition

- `cargo publish --dry-run` passes with no warnings.
- `cargo test --lib`, `cargo clippy --all-targets -- -D warnings`, and `cargo doc --no-deps` pass in CI.
- README/CHANGELOG present; docs and examples reflect the current API.
- Supported hardware/features are explicit and ESP32-P4 is documented as experimental/hidden.
- esp-hal and embassy-net examples compile for the target toolchain.
- Integration tests are documented, reproducible, and run cleanly.
