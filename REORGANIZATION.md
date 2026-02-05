# Crate Reorganization Summary

This document records the completed reorganization of the `ph-esp32-mac` crate.
Forward-looking work is tracked in `PUBLISHABILITY_ROADMAP.md` and takes
precedence if anything diverges.

---

## Current Layout

```
src/
├── driver/           # Core driver API (Emac, config, errors, filtering, flow)
├── hal/              # Clock/reset/MDIO/GPIO abstractions
├── phy/              # PHY drivers and traits
├── integration/      # smoltcp, embassy-net-driver, esp-hal facades
├── sync/             # SharedEmac and async waker support
├── internal/         # Registers, DMA descriptors, constants (pub(crate))
├── testing/          # Host test utilities (cfg(test))
└── lib.rs            # Public API and re-exports
```

## What Changed (High Level)

- Internal hardware details (registers, DMA descriptors, PHY register tables)
  live under `src/internal` and are not part of the public API.
- Public APIs are surfaced via `lib.rs` re-exports and `driver/*` modules.
- Integration surfaces (`smoltcp`, `esp-hal`, `embassy-net-driver`, async) live
  under `src/integration` and `src/sync`.
- Host testing utilities moved to `src/testing`.

## Public API Guidance

- Treat items re-exported in `lib.rs` as the stable API surface.
- Do not depend on `internal/*` from external code.
- This project does not maintain deprecated compatibility layers; breaking
  changes are allowed between releases when needed.

## Status

Reorganization is complete. Any new structure work should be captured in
`PUBLISHABILITY_ROADMAP.md`.
