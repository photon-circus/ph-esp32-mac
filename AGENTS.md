# AI Agent Guidelines

This document provides guidance for AI agents (such as GitHub Copilot, Claude, ChatGPT, or similar) working on the ESP32 EMAC driver project. Following these guidelines ensures consistent, high-quality contributions that integrate well with the existing codebase.

---

## Table of Contents

1. [Project Overview](#project-overview)
2. [Key Documents](#key-documents)
3. [Code Standards](#code-standards)
4. [Documentation Requirements](#documentation-requirements)
5. [Testing Requirements](#testing-requirements)
6. [Common Tasks](#common-tasks)
7. [Things to Avoid](#things-to-avoid)
8. [Verification Steps](#verification-steps)

---

## Project Overview

This is a `no_std` Rust driver for the ESP32 EMAC (Ethernet MAC) peripheral. The driver supports:

- **Target Hardware**: ESP32 (xtensa-esp32-none-elf), with ESP32-P4 support planned
- **PHY Support**: LAN8720A (primary), generic PHY fallback
- **Interfaces**: RMII (primary), MII (secondary)
- **Stack Integration**: smoltcp-compatible

### Architecture Summary

```
┌──────────────────────────────────────────────────────┐
│                    Application                       │
├──────────────────────────────────────────────────────┤
│                 smoltcp / embassy-net                │
├──────────────────────────────────────────────────────┤
│  EmacController (mac.rs)                             │
│    ├── DmaEngine (dma.rs)                            │
│    ├── PHY Driver (phy/lan8720a.rs)                  │
│    └── Register Access (register/*.rs)               │
├──────────────────────────────────────────────────────┤
│                  ESP32 Hardware                      │
└──────────────────────────────────────────────────────┘
```

---

## Key Documents

**Always consult these documents before making changes:**

| Document | Purpose |
|----------|---------|
| [DESIGN.md](DESIGN.md) | Architecture decisions, module structure, design rationale |
| [TESTING.md](TESTING.md) | Test strategy, coverage goals, how to run tests |
| [DOCUMENTATION_STANDARDS.md](DOCUMENTATION_STANDARDS.md) | **Mandatory** - All documentation must follow these standards |
| [Cargo.toml](Cargo.toml) | Dependencies, features, crate metadata |

### Documentation Standards (Critical)

All code contributions **must** follow the standards defined in [DOCUMENTATION_STANDARDS.md](DOCUMENTATION_STANDARDS.md). Key requirements:

- Every public item requires doc comments
- Use `///` for items, `//!` for modules
- Include `# Safety` for all unsafe code
- Include `# Errors` for fallible functions
- Follow the prescribed section order

---

## Code Standards

### Rust Style

```rust
// ✅ GOOD: Follow these patterns
#![no_std]  // This is a no_std crate - no std library

use core::cell::RefCell;  // Use core::, not std::
use crate::error::Result;  // Use crate-local Result type

/// Document all public items.
pub fn example() -> Result<()> {
    // ...
}
```

### No-Std Constraints

This crate is `no_std` with `no_alloc`:

- ❌ No `std::` imports (use `core::` instead)
- ❌ No `Vec`, `String`, `Box`, `HashMap` in production code
- ❌ No heap allocation in production code
- ✅ Use fixed-size arrays with const generics
- ✅ Use static allocation for global state
- ✅ Tests may use `std` through `test_utils.rs`

### Register Access

```rust
// All hardware register access goes through register/*.rs modules
use crate::register::dma::DmaRegs;
use crate::register::mac::MacRegs;
use crate::register::ext::ExtRegs;

// Always use the type-safe wrappers
DmaRegs::set_bus_mode(value);  // ✅ Good
unsafe { write_reg(DMA_BASE + offset, value) };  // ⚠️ Only in register module
```

### Error Handling

```rust
// Use the crate's error types
use crate::error::{Error, DmaError, IoError, Result};

// Return appropriate error variants
pub fn transmit(&mut self, data: &[u8]) -> Result<()> {
    if data.len() > BUF_SIZE {
        return Err(Error::Dma(DmaError::FrameTooLarge));
    }
    // ...
}
```

---

## Documentation Requirements

### Minimum Requirements

Every code change must include:

1. **Doc comments** on all new public items
2. **Inline comments** explaining non-obvious logic
3. **Safety comments** for all `unsafe` blocks
4. **Updates** to relevant .md files if architecture changes

### Doc Comment Template

```rust
/// Brief one-line summary.
///
/// Extended description if needed.
///
/// # Arguments
///
/// * `param` - Description
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// * `ErrorType` - When this occurs
///
/// # Examples
///
/// ```ignore
/// let result = function();
/// ```
pub fn function(param: Type) -> Result<Output> {
    // ...
}
```

Refer to [DOCUMENTATION_STANDARDS.md](DOCUMENTATION_STANDARDS.md) for complete guidelines.

---

## Testing Requirements

### Host Tests (Required)

All logic that doesn't require hardware must have unit tests:

```bash
# Run all tests
cargo test --lib

# Run specific module tests
cargo test --lib dma
cargo test --lib phy::lan8720a

# Check coverage
cargo llvm-cov --lib
```

### Test File Organization

- Tests live in `#[cfg(test)] mod tests { }` at the bottom of each file
- Mock utilities go in `src/test_utils.rs`
- Integration tests go in `integration_tests/` directory

### Available Mocks

```rust
use crate::test_utils::{MockMdioBus, MockDelay, MockDescriptor};

#[test]
fn test_example() {
    let mut mdio = MockMdioBus::new();
    mdio.setup_lan8720a(0);  // Configure for LAN8720A
    mdio.simulate_link_up_100_fd(0);  // Simulate link up
    
    // Test your code...
}
```

### Coverage Expectations

| Module Type | Minimum Coverage |
|-------------|------------------|
| Logic modules (config, error) | 80%+ |
| PHY drivers | 75%+ |
| DMA/descriptor logic | 65%+ |
| Hardware register access | 0% (hardware-only) |

Refer to [TESTING.md](TESTING.md) for detailed test documentation.

---

## Common Tasks

### Adding a New PHY Driver

1. Create `src/phy/<phy_name>.rs`
2. Implement the same interface as `Lan8720a`
3. Add tests using `MockMdioBus`
4. Document all register addresses and bit fields
5. Update `src/phy/mod.rs` exports
6. Update DESIGN.md if architecture changed

### Adding a New Register

1. Add constant definitions in appropriate `register/*.rs` file
2. Document bit fields with tables
3. Add accessor functions if needed
4. Reference ESP32 Technical Reference Manual

### Adding a New Error Type

1. Add variant to appropriate error enum in `error.rs`
2. Implement `as_str()` for the variant
3. Add test for the new variant
4. Update any functions that should return this error

### Fixing a Bug

1. First, write a test that reproduces the bug
2. Fix the bug
3. Verify the test passes
4. Update documentation if behavior changed

---

## Things to Avoid

### Code Anti-Patterns

```rust
// ❌ DON'T: Use std
use std::vec::Vec;

// ❌ DON'T: Allocate on heap
let buffer = vec![0u8; 1500];

// ❌ DON'T: Use unwrap in production code
let value = register.read().unwrap();

// ❌ DON'T: Access registers directly outside register module
let value = unsafe { *(0x3FF69000 as *const u32) };

// ❌ DON'T: Leave unsafe blocks without safety comments
unsafe {
    do_something_dangerous();
}
```

### Documentation Anti-Patterns

```rust
// ❌ DON'T: Skip documentation on public items
pub fn important_function() { }

// ❌ DON'T: Write trivial comments
// Increment the counter
counter += 1;

// ❌ DON'T: Leave TODOs without context
// TODO: fix this
```

### Testing Anti-Patterns

```rust
// ❌ DON'T: Test names that don't describe the test
#[test]
fn test1() { }

// ❌ DON'T: Tests without assertions
#[test]
fn test_something() {
    let x = calculate();
    // No assertions!
}

// ❌ DON'T: Ignore test failures
#[test]
#[ignore]
fn broken_test() { }  // Fix it instead
```

---

## Verification Steps

### Before Submitting Changes

Run these commands to verify your changes:

```bash
# 1. Format code
cargo fmt

# 2. Run clippy
cargo clippy -- -D warnings

# 3. Run tests
cargo test --lib

# 4. Check documentation builds
cargo doc --no-deps

# 5. Check coverage (optional but recommended)
cargo llvm-cov --lib
```

### Checklist

- [ ] Code compiles without warnings
- [ ] All tests pass
- [ ] New public items have doc comments
- [ ] Unsafe code has safety comments
- [ ] No `std` imports in production code
- [ ] Documentation follows [DOCUMENTATION_STANDARDS.md](DOCUMENTATION_STANDARDS.md)
- [ ] TESTING.md updated if tests added
- [ ] DESIGN.md updated if architecture changed

---

## Quick Reference

### File Locations

| Type | Location |
|------|----------|
| Main library | `src/lib.rs` |
| DMA engine | `src/dma.rs` |
| Descriptors | `src/descriptor/*.rs` |
| PHY drivers | `src/phy/*.rs` |
| Register access | `src/register/*.rs` |
| Configuration | `src/config.rs` |
| Error types | `src/error.rs` |
| Test utilities | `src/test_utils.rs` |
| Integration tests | `integration_tests/` |

### Important Types

| Type | Purpose |
|------|---------|
| `DmaEngine<RX, TX, SIZE>` | Manages DMA descriptor rings |
| `TxDescriptor` / `RxDescriptor` | DMA descriptor structures |
| `Lan8720a` | LAN8720A PHY driver |
| `EmacConfig` | Configuration builder |
| `Error` / `Result` | Error handling |
| `InterruptStatus` | Parsed interrupt flags |

### ESP32 Resources

- [ESP32 Technical Reference Manual](https://www.espressif.com/sites/default/files/documentation/esp32_technical_reference_manual_en.pdf) - Chapter 10: Ethernet MAC
- [ESP-IDF EMAC Driver](https://github.com/espressif/esp-idf/tree/master/components/esp_eth) - Reference implementation
- [LAN8720A Datasheet](https://www.microchip.com/wwwproducts/en/LAN8720A) - PHY documentation
