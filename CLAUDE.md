# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a `no_std`, `no_alloc` Rust driver for the ESP32 Ethernet MAC (EMAC) controller. It provides a bare-metal implementation based on the Synopsys DesignWare MAC (DWMAC) IP core, supporting 10/100 Mbps Ethernet with DMA-based packet transfer.

Key constraints:
- No standard library (`no_std`)
- No heap allocation (all memory is statically allocated via const generics)
- Use `core::` instead of `std::` for all imports

## Build and Test Commands

```bash
# Format all code
cargo fmt

# Run host unit tests
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
+--------------------------------------------------------------+
| Application Layer                                            |
| smoltcp / embassy-net / raw Ethernet processing               |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| Integration Layer                                             |
| integration/{smoltcp, embassy_net, esp_hal}                   |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| Sync Layer                                                    |
| sync::{SharedEmac, AsyncEmacState, AsyncEmacExt}              |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| Driver Layer                                                  |
| driver::{emac, config, interrupt, filtering, flow}            |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| HAL Layer                                                     |
| hal::{clock, reset, mdio, gpio}                               |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| Internal Layer                                                |
| internal::{register, dma, phy_regs, constants}                |
+--------------------------------------------------------------+
                              |
                              v
+--------------------------------------------------------------+
| ESP32 Hardware                                                |
| EMAC MAC + DMA + EXT + External PHY                           |
+--------------------------------------------------------------+
```

### Module Organization

| Path | Purpose |
|------|---------|
| `src/driver/` | Main EMAC driver, config, errors, interrupts |
| `src/hal/` | Hardware abstraction: clock, MDIO, reset, GPIO |
| `src/phy/` | PHY drivers (LAN8720A, generic helpers) |
| `src/integration/` | smoltcp, embassy-net-driver, esp-hal facades |
| `src/sync/` | SharedEmac and async waker support |
| `src/internal/` | Private implementation: DMA engine, descriptors, registers |
| `src/testing/` | Host test utilities (cfg(test)) |

### Key Types

- `Emac<RX, TX, BUF>` - Main driver, owns all DMA memory
- `EmacConfig` - Builder-style configuration
- `EmacBuilder` - esp-hal facade builder
- `EmacPhyBundle` - esp-hal helper for PHY init/link
- `MdioController` - PHY register access via MDIO/MDC
- `Lan8720a` / `Lan8720aWithReset` - LAN8720A PHY drivers
- `SharedEmac` - Critical-section protected wrapper for ISR access
- `AsyncEmacState` / `AsyncEmacExt` - Async wakers and async TX/RX
- `EmbassyEmac` / `EmbassyEmacState` - embassy-net-driver integration
- `InterruptStatus` - Parsed interrupt status flags

## Feature Flags

```toml
esp32 = []                    # Target ESP32 (default)
esp32p4 = []                  # Experimental placeholder (not supported)
defmt = ["dep:defmt"]         # defmt debug output
log = ["dep:log"]             # log facade support
smoltcp = ["dep:smoltcp"]     # smoltcp Device trait
critical-section = [...]      # SharedEmac wrapper
async = ["critical-section"]  # Async/await support
esp-hal = [...]               # esp-hal facade helpers
embassy-net = [...]           # embassy-net-driver integration
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

ESP32 RMII pins are fixed and cannot be remapped:
- TX_EN: GPIO21, TXD0: GPIO19, TXD1: GPIO22
- RX_DV: GPIO27, RXD0: GPIO25, RXD1: GPIO26
- REF_CLK: GPIO0, MDIO: GPIO18, MDC: GPIO23

Memory budget (default 10 RX/TX, 1600 byte buffers): ~32 KB
