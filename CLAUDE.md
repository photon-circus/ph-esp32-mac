# CLAUDE.md

Minimal guidance for assistants working in this repo.

## Read First

- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTING.md`
- `docs/DOCUMENTATION_STANDARDS.md`

## Constraints

- `no_std`, `no_alloc`: no heap allocations in production code.
- Use `core::` imports, not `std::`.
- All public items must be documented.
- Every `unsafe` block needs a `// SAFETY:` comment.

## Scope

- ESP32 only (xtensa-esp32-none-elf).
- `esp32p4` is experimental/hidden.
