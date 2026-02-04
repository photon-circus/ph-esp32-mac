# Crate Reorganization Plan

This document outlines the multi-phase reorganization plan for the `ph-esp32-mac` crate.
The goal is to establish clear separation between the public API and internal implementation details.

## Overview

The reorganization follows these principles:

1. **Public API Stability**: Types and functions exposed via `pub use` in `lib.rs` form the stable API
2. **Internal Implementation**: Low-level details move under `internal/` as `pub(crate)` only
3. **Backward Compatibility**: Deprecated re-exports provide migration path for existing users
4. **Documentation**: Clean public API surface with hidden internal modules

---

## Phase 1: Create `internal/` Module ✅ COMPLETED

**Status:** Completed

**Changes Made:**
- Created `src/internal/mod.rs` container module
- Moved `src/register/` → `src/internal/register/`
- Moved `src/constants.rs` → `src/internal/constants.rs`
- Updated all internal imports to use `crate::internal::register::*` and `crate::internal::constants::*`
- Updated register macros to use `$crate::internal::register::` paths
- Added deprecated re-exports in `lib.rs` for backward compatibility

**New Structure:**
```
src/
├── internal/
│   ├── mod.rs           # Container with pub(crate) exports
│   ├── constants.rs     # Magic numbers and configuration constants
│   └── register/        # Raw memory-mapped register definitions
│       ├── mod.rs
│       ├── dma.rs
│       ├── ext.rs
│       └── mac.rs
└── ...
```

**Migration Path:**
The deprecated modules in `lib.rs` allow existing code to continue working:
```rust
// Old (deprecated but still works)
use ph_esp32_mac::constants::MAX_FRAME_SIZE;
use ph_esp32_mac::register::dma::DmaRegs;

// New (preferred)
use ph_esp32_mac::MAX_FRAME_SIZE;  // Via pub use in lib.rs
use ph_esp32_mac::DmaRegs;          // Via pub use in lib.rs
```

---

## Phase 2: HAL Module Cleanup ✅ COMPLETED

**Status:** Completed

**Goal:** Restructure the `hal/` module for cleaner public exports.

### Changes Made

1. **Created `internal/phy_registers.rs`:**
   - Moved IEEE 802.3 PHY register definitions from `hal/mdio.rs`
   - Contains: `phy_reg`, `bmcr`, `bmsr`, `anar`, `anlpar`, `aner` modules
   - Complete register documentation with bit definitions

2. **Created `internal/gpio_pins.rs`:**
   - Moved ESP32 GPIO pin constants from `hal/gpio.rs`
   - Contains `esp32` module with all RMII pin assignments
   - Placeholder for ESP32-P4 support

3. **Updated `hal/mdio.rs`:**
   - Added deprecated re-exports for backward compatibility
   - Internal functions now use `internal::phy_registers`
   - Public API unchanged (`MdioController`, `MdioBus`, etc.)

4. **Updated `hal/gpio.rs`:**
   - Added deprecated re-export for `esp32_gpio` module
   - Documentation preserved

5. **Updated PHY modules:**
   - `phy/mod.rs`: Now re-exports from `internal::phy_registers`
   - `phy/generic.rs`: Uses internal imports
   - `phy/lan8720a.rs`: Uses internal imports
   - `test_utils.rs`: Uses internal imports

### New Structure
```
src/internal/
├── mod.rs              # Container with pub(crate) exports
├── constants.rs        # Phase 1
├── register/           # Phase 1
├── phy_registers.rs    # NEW - IEEE 802.3 register definitions
└── gpio_pins.rs        # NEW - GPIO pin assignments
```

### Tasks Completed
- [x] Move PHY register definitions to internal module
- [x] Move GPIO pin constants to internal module
- [x] Add deprecated re-exports in hal/mdio.rs
- [x] Add deprecated re-export in hal/gpio.rs
- [x] Update PHY modules to use internal imports
- [x] Verify 308 tests pass

---

## Phase 3: DMA/Descriptor Encapsulation

**Status:** ✅ Complete

**Goal:** Hide low-level DMA details while preserving public types.

### Current Structure
```
src/
├── dma.rs           # DmaEngine, DescriptorRing
├── descriptor/
│   ├── mod.rs       # Re-exports, constants
│   ├── rx.rs        # RxDescriptor
│   └── tx.rs        # TxDescriptor
└── internal/
    └── descriptor_bits.rs  # NEW: Bit field constants
```

### Completed Changes

1. **Created `internal/descriptor_bits.rs`**:
   - Organized constants into submodules: `rdes0`, `rdes1`, `rdes4`, `tdes0`, `tdes1`, `checksum_mode`
   - All constants documented with doc comments
   - Added `#![allow(dead_code)]` for unused constants

2. **Updated `descriptor/rx.rs`**:
   - Imports from `internal::descriptor_bits::{rdes0, rdes1, rdes4}`
   - Internal constants use `_INT` suffix to avoid conflicts
   - All original constants re-exported with `#[deprecated]` attribute
   - Doc comments added to all deprecated re-exports
   - Test module uses `#[allow(deprecated)]`

3. **Updated `descriptor/tx.rs`**:
   - Imports from `internal::descriptor_bits::{tdes0, tdes1}`
   - Internal constants use `_INT` suffix to avoid conflicts
   - All original constants re-exported with `#[deprecated]` attribute
   - Deprecated `checksum_mode` module re-exports from internal
   - Doc comments added to all deprecated re-exports
   - Test module uses `#[allow(deprecated)]`

4. **Updated `internal/mod.rs`**:
   - Added `pub(crate) mod descriptor_bits;`
   - Updated module documentation

### Public API (preserved)
- `DmaEngine<RX, TX, SIZE>` - Main engine
- `DescriptorRing<D, N>` - Ring buffer
- `RxDescriptor`, `TxDescriptor` - Descriptor types
- `VolatileCell<T>` - Volatile access wrapper
- `DESC_OWN`, `DESC_ALIGNMENT` - Shared constants

### Deprecated (backward compatible)
- All `RDES0_*`, `RDES1_*`, `RDES4_*` constants in `descriptor/rx.rs`
- All `TDES0_*`, `TDES1_*` constants in `descriptor/tx.rs`
- `descriptor::tx::checksum_mode` module

### Test Results
- All 308 tests pass
- Clippy clean (no warnings)

---

## Phase 4: PHY Driver Architecture

**Status:** ✅ Complete

**Goal:** Clean PHY driver abstraction with clear extension points.

### Changes Made

1. **Created `internal/lan8720a_regs.rs`**:
   - `phy_id` module: PHY identifier constants (`ID`, `MASK`)
   - `timing` module: Internal timing constants (`RESET_MAX_ATTEMPTS`, etc.)
   - `reg` module: Vendor-specific register addresses (`MCSR`, `SMR`, etc.)
   - `mcsr` module: Mode Control/Status Register bits
   - `smr` module: Special Modes Register bits
   - `scsir` module: Special Control/Status Indication Register bits
   - `isr` module: Interrupt Source Register bits
   - `pscsr` module: PHY Special Control/Status Register bits

2. **Updated `internal/mod.rs`**:
   - Added `lan8720a_regs` module export
   - Updated module documentation

3. **Updated `phy/lan8720a.rs`**:
   - Import constants from `internal::lan8720a_regs`
   - Public constants re-export from internal module for backward compatibility
   - All submodules (`reg`, `mcsr`, `smr`, `scsir`, `isr`, `pscsr`) now delegate to internal

### Current Structure
```
src/phy/
├── mod.rs           # Public re-exports (PhyDriver, LinkStatus, etc.)
├── generic.rs       # PhyDriver trait, LinkStatus, PhyCapabilities
└── lan8720a.rs      # LAN8720A implementation (uses internal constants)

src/internal/
├── lan8720a_regs.rs # NEW: LAN8720A vendor-specific registers
└── ...other modules...
```

### Public API (stable)
- `PhyDriver` trait - Extension point for new PHY drivers
- `LinkStatus` struct - Speed/duplex information
- `PhyCapabilities` struct - PHY feature support
- `Lan8720a` struct - LAN8720A driver without reset pin
- `Lan8720aWithReset<R>` struct - LAN8720A driver with reset pin
- `LAN8720A_PHY_ID`, `LAN8720A_PHY_ID_MASK` constants
- `reg`, `mcsr`, `smr`, `scsir`, `isr`, `pscsr` modules (re-exports)

### Internal (implementation details)
- `internal::lan8720a_regs` - LAN8720A register definitions
- Timing constants (reset attempts, pulse duration, etc.)

### Extension Point for New PHY Drivers

To add a new PHY driver:

1. Create `src/phy/my_phy.rs`
2. Implement the `PhyDriver` trait
3. Create `src/internal/my_phy_regs.rs` for vendor-specific registers
4. Use `ieee802_3` helper functions from `generic.rs` for standard operations
5. Export from `src/phy/mod.rs`

Example:
```rust
use crate::error::Result;
use crate::hal::mdio::MdioBus;
use crate::phy::generic::{ieee802_3, LinkStatus, PhyCapabilities, PhyDriver};

pub struct MyPhy {
    addr: u8,
}

impl PhyDriver for MyPhy {
    fn address(&self) -> u8 { self.addr }
    
    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        self.soft_reset(mdio)?;
        self.enable_auto_negotiation(mdio)
    }
    
    fn soft_reset<M: MdioBus>(&mut self, mdio: &mut M) -> Result<()> {
        ieee802_3::soft_reset(mdio, self.addr, 1000)
    }
    
    // ... implement remaining methods
}
```

### Test Results
- All 308 tests pass
- Clippy clean (no warnings)

---

## Phase 5: Feature Organization & Documentation

**Status:** Not Started

**Goal:** Clean up feature-gated modules and improve documentation.

### Current Features
```toml
[features]
default = ["esp32"]
esp32 = []
esp32p4 = []
defmt = ["dep:defmt"]
smoltcp = ["dep:smoltcp"]
critical-section = ["dep:critical-section"]
async = ["critical-section", "dep:embedded-hal-async"]
esp-hal = ["dep:esp-hal", "dep:esp-hal-procmacros"]
```

### Proposed Changes

1. **Feature-gated module organization**:
   ```rust
   // lib.rs structure
   
   // Core modules (always available)
   pub mod config;
   pub mod dma;
   pub mod error;
   pub mod hal;
   pub mod mac;
   pub mod phy;
   
   // Internal (pub(crate) only)
   mod internal;
   
   // Feature-gated modules
   #[cfg(feature = "smoltcp")]
   pub mod smoltcp;
   
   #[cfg(feature = "critical-section")]
   pub mod sync;
   
   #[cfg(feature = "critical-section")]
   pub mod sync_primitives;
   
   #[cfg(feature = "async")]
   pub mod asynch;
   
   #[cfg(feature = "esp-hal")]
   pub mod esp_hal;
   ```

2. **Documentation improvements**:
   - Add module-level examples
   - Document feature combinations
   - Add migration guide for deprecated items
   - Improve error documentation

3. **Prelude module** (optional):
   ```rust
   /// Commonly used types for convenient import
   pub mod prelude {
       pub use crate::{
           Emac, EmacConfig, EmacDefault,
           Lan8720a, PhyDriver, LinkStatus,
           Error, Result,
       };
       
       #[cfg(feature = "critical-section")]
       pub use crate::SharedEmac;
       
       #[cfg(feature = "async")]
       pub use crate::AsyncEmacExt;
   }
   ```

### Tasks
- [ ] Review feature combinations for conflicts
- [ ] Add prelude module
- [ ] Document each feature in lib.rs
- [ ] Add migration guide for 0.1.x → 0.2.x
- [ ] Update TESTING.md with feature-specific tests

---

## Migration Guide

### For Users of `crate::constants`

**Before (0.1.x):**
```rust
use ph_esp32_mac::constants::{MAX_FRAME_SIZE, MTU};
```

**After (0.2.x):**
```rust
// Constants are re-exported at crate root
use ph_esp32_mac::{MAX_FRAME_SIZE, MTU};
```

### For Users of `crate::register`

**Before (0.1.x):**
```rust
use ph_esp32_mac::register::dma::DmaRegs;
use ph_esp32_mac::register::mac::MacRegs;
```

**After (0.2.x):**
```rust
// Register types are re-exported at crate root
use ph_esp32_mac::{DmaRegs, MacRegs, ExtRegs};
```

### Deprecation Timeline

| Version | Status |
|---------|--------|
| 0.1.x   | Full compatibility |
| 0.2.x   | Deprecated re-exports with warnings |
| 0.3.x   | Remove deprecated re-exports |

---

## Testing Strategy

Each phase must maintain:

1. **All existing tests pass** (currently 308 tests)
2. **No clippy warnings** (`cargo clippy -- -D warnings`)
3. **Documentation builds** (`cargo doc --no-deps`)
4. **Coverage maintained** (target: 65%+ overall)

### Verification Commands

```bash
# Full test suite
cargo test --lib --features "smoltcp,critical-section,async"

# Clippy
cargo clippy -- -D warnings

# Documentation
cargo doc --no-deps

# Coverage (requires llvm-cov)
cargo llvm-cov --lib
```

---

## Progress Tracking

| Phase | Description | Status | Tests |
|-------|-------------|--------|-------|
| 1 | Create `internal/` module | ✅ Complete | 308 passing |
| 2 | HAL module cleanup | ✅ Complete | 308 passing |
| 3 | DMA/Descriptor encapsulation | ⏳ Not started | - |
| 4 | PHY driver architecture | ⏳ Not started | - |
| 5 | Feature organization & documentation | ⏳ Not started | - |

---

## Notes

- Phase 1 establishes the pattern for subsequent phases
- Each phase should be independently committable
- Maintain backward compatibility until 0.3.x release
- Document all breaking changes in CHANGELOG.md
