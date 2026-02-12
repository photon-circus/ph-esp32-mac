# AI Agent Notes

Keep changes aligned with the project’s design and documentation standards.

## Primary References

- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTING.md`
- `docs/DOCUMENTATION_STANDARDS.md`

## Core Constraints

- `no_std`, `no_alloc`: use `core::`, no heap allocations in production code.
- All public items require doc comments.
- Every `unsafe` block requires a `// SAFETY:` comment and rationale.

## Scope

- Target hardware: **ESP32 only**. `esp32p4` is experimental and hidden.
- Prefer the esp-hal facade + WT32-ETH01 helpers for canonical examples.
