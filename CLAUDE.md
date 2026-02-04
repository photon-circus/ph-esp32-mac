# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a `no_std`, `no_alloc` Rust driver for the ESP32 Ethernet MAC (EMAC) controller. It provides a bare-metal implementation based on the Synopsys DesignWare MAC (DWMAC) IP core, supporting 10/100 Mbps Ethernet with DMA-based packet transfer.

**Key constraints:**
- No standard library (`no_std`)
- No heap allocation - all memory statically allocated via const generics
- Use `core::` instead of `std::` for all imports

## Build and Test Commands

```bash
# Format all code
cargo fmt

# Run all host unit tests (299 tests)
cargo test --lib

# Run specific module tests
cargo test --lib dma
cargo test --lib phy::lan8720a
cargo test --lib descriptor

# Run clippy
cargo clippy -- -D warnings

# Build documentation
cargo doc --no-deps

# Run code coverage (requires cargo-llvm-cov)
cargo llvm-cov --lib

# Integration tests (build only)
cargo int-build

# Integration tests (flash + run; requires hardware)
cargo int
```

### Building for ESP32 Hardware

```bash
cd integration_tests
cargo build --release

# Flash and monitor (requires espflash)
espflash flash target/xtensa-esp32-none-elf/release/wt32_eth01 --monitor
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Application / smoltcp                      │
└─────────────────────────────────────────────────────────────────┘
                                │
┌───────────────────────────────┴───────────────────────────────┐
│  driver/emac.rs - Emac<RX_BUFS, TX_BUFS, BUF_SIZE>            │
│    └── driver/config.rs, error.rs, interrupt.rs, filtering.rs │
└───────────────────────────────────────────────────────────────┘
        │                  │                   │
┌───────┴──────┐  ┌────────┴────────┐  ┌───────┴───────┐
│ internal/dma │  │    hal/mdio     │  │   phy/        │
│ DmaEngine    │  │ MdioController  │  │   Lan8720a    │
│ TxDescriptor │  │ PhyStatus       │  │   PhyDriver   │
│ RxDescriptor │  │                 │  │               │
└───────┬──────┘  └────────┬────────┘  └───────────────┘
        │                  │
┌───────┴──────────────────┴───────────────────────────┐
│  internal/register/ - DmaRegs, MacRegs, ExtRegs      │
│  Memory-mapped register access via volatile ops      │
└──────────────────────────────────────────────────────┘
```

### Module Organization

| Path | Purpose |
|------|---------|
| `src/driver/` | Main EMAC driver, config, errors, interrupts |
| `src/hal/` | Hardware abstraction: clock, MDIO, reset, GPIO |
| `src/phy/` | PHY drivers (LAN8720A, generic) |
| `src/internal/` | Private implementation: DMA engine, descriptors, registers |
| `src/sync/` | Thread-safe wrappers (SharedEmac, async support) |
| `src/integration/` | smoltcp and esp-hal integrations |

### Key Types

- `Emac<RX_BUFS, TX_BUFS, BUF_SIZE>` - Main driver, owns all DMA memory
- `DmaEngine` - Manages TX/RX descriptor rings
- `TxDescriptor` / `RxDescriptor` - DMA descriptor structures (32 bytes each)
- `MdioController` - PHY register access via MDIO/MDC
- `Lan8720a` - LAN8720A PHY driver implementing `PhyDriver` trait
- `SharedEmac` - Critical-section protected wrapper for ISR access
- `InterruptStatus` - Parsed interrupt status flags

## Feature Flags

```toml
esp32 = []                    # Target ESP32 (default)
esp32p4 = []                  # Target ESP32-P4
defmt = ["dep:defmt"]         # defmt debug output
smoltcp = ["dep:smoltcp"]     # smoltcp Device trait
critical-section = [...]      # SharedEmac wrapper
esp-hal = [...]               # esp-hal integration
async = ["critical-section"]  # Async/await support
```

## Code Patterns

### Register Access
All hardware register access goes through `internal/register/*.rs`:
```rust
use crate::internal::register::dma::DmaRegs;
DmaRegs::set_bus_mode(value);  // Type-safe wrapper
```

### Error Handling
Three error domains with a unified `Error` enum:
```rust
use crate::driver::error::{ConfigError, DmaError, IoError, Error, Result};
```

### Unsafe Code
Every `unsafe` block requires a `// SAFETY:` comment explaining the invariants.

### Testing
Tests use mocks from `src/testing/`:
- `MockMdioBus` - Simulates PHY registers
- `MockDelay` - Tracks delay calls
- `MockDescriptor` - Simulates DMA descriptor behavior

## Documentation

All public items require doc comments. See DOCUMENTATION_STANDARDS.md for:
- Section order: Summary, Description, Arguments, Returns, Errors, Safety, Examples
- Use `# Safety` for all unsafe functions
- Use `# Errors` for fallible functions

## Hardware Notes

**ESP32 RMII pins are fixed** - cannot be remapped:
- TX_EN: GPIO21, TXD0: GPIO19, TXD1: GPIO22
- RX_DV: GPIO27, RXD0: GPIO25, RXD1: GPIO26
- REF_CLK: GPIO0, MDIO: GPIO18, MDC: GPIO23

**Memory budget** (default 10 RX/TX, 1600 byte buffers): ~32 KB
