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
- CI workflows are in place for format/clippy/tests/docs (target checks are still being tuned for xtensa availability).
- Documentation was recently consolidated, but packaging metadata and publish artifacts remain incomplete.

---

## Current Gaps

- **Packaging metadata**: No root `README.md`, no `CHANGELOG.md`, and `Cargo.toml` is missing `repository`, `documentation`, `homepage`, and docs.rs metadata.
- **Public API audit**: No explicit inventory of public surface area; some APIs still feel “internal-first” rather than idiomatic embedded-Rust facades.
- **Feature docs**: Feature gating for `esp32p4` is enforced but still needs “experimental/hidden from docs” treatment and clearer doc(cfg) coverage.
- **Safety and documentation polish**: Need a structured unsafe-audit pass to guarantee `# Safety` sections and `SAFETY:` comments everywhere required.
- **Release mechanics**: No release checklist or dry-run publish validation tracked in the roadmap.

---

## Scope and Targets

- **Publish target**: crates.io
- **Supported hardware for next release**: ESP32 only
- **Experimental / hidden**: ESP32-P4 (not implemented yet; hide from docs)
- **Primary consumer**: `esp-hal` 1.0.0
- **Secondary integration**: `embassy-net` 0.7.0
- **MSRV**: Rust 1.92.0 (locked)

---

## Sprint Plan

### Sprint 0 - Packaging and Release Hygiene

**Goal**: Make the crate publishable from a packaging perspective.

**Work items**
- Add root `README.md` (aligned with `DOCUMENTATION_STANDARDS.md`) and set `readme = "README.md"`.
- Add `repository`, `documentation`, `homepage`, and `package.metadata.docs.rs`.
- Add `CHANGELOG.md` and `RELEASE.md` with a publish checklist.
- Add `exclude` list to `Cargo.toml` for non-package artifacts (`target/`, `.idea/`, `DESIGN_old.md`, `REORGANIZATION.md`, etc.).
- note the repo url is https://github.com/photon-circus/ph-esp32-mac
- license is apache 2.0
- note issues with ci fmt failing on examples
- note issues with ci doc failing because of documentation quality issues

**Exit criteria**
- `cargo package --list` is clean and includes only intended artifacts.
- `cargo publish --dry-run` passes locally.

---

### Sprint 1 - Public API Audit and Embedded-Rust Idioms

**Goal**: Ensure the public surface is idiomatic, minimal, and stable for embedded users (esp-hal first).

**Work items**
- Inventory all public items across `driver`, `integration`, and `sync` modules; categorize as stable vs internal.
- Tighten re-exports to reduce surface bloat; expose a clean top-level facade.
- Verify API names and patterns align with embedded-Rust norms:
  - Builder patterns, explicit `Result` types, `no_std` conventions.
  - Minimize unsafe exposure and raw pointers in public APIs.
- Review feature gating and doc(cfg) coverage:
  - `esp-hal`, `embassy-net`, `smoltcp`, `async`, `critical-section`.
  - Ensure `esp32p4` is documented as experimental and hidden from docs.
- Run an unsafe-audit pass:
  - All `unsafe` blocks have `SAFETY:` comments.
  - All unsafe public APIs have a `# Safety` section.

**Exit criteria**
- A short “Public API report” section added to this roadmap (or a new `API.md`).
- All public APIs follow documented style and safety requirements.
- No clippy warnings for `missing_docs` or unsafe documentation in public items.

---

### Sprint 2 - Documentation and Examples Polish

**Goal**: Present a clear, minimal integration story with current examples and docs.

**Work items**
- Update root docs and examples README with “happy path” snippets:
  - esp-hal bring-up
  - embassy-net bring-up
  - smoltcp usage
- Verify all docs show correct crate name and API (no legacy references).
- Ensure examples README and integration tests README match current CLI aliases and example layout.
- Add a concise memory/footprint section (DMA buffer sizes, defaults).

**Exit criteria**
- Docs build cleanly with `cargo doc --no-deps`.
- Example READMEs are accurate, minimal, and up-to-date.

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
