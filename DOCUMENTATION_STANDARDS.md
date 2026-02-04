# Documentation Standards

This document defines the documentation standards for the ESP32 EMAC driver project. All code, comments, and documentation files must follow these guidelines to ensure consistency, maintainability, and clarity.

---

## Table of Contents

1. [General Principles](#general-principles)
2. [Rust Doc Comments](#rust-doc-comments)
3. [Code Comments](#code-comments)
4. [Markdown Files](#markdown-files)
5. [Module Documentation](#module-documentation)
6. [API Documentation](#api-documentation)
7. [Error Documentation](#error-documentation)
8. [Safety Documentation](#safety-documentation)
9. [Examples and Code Snippets](#examples-and-code-snippets)
10. [Tables and Diagrams](#tables-and-diagrams)
11. [Changelog and Versioning](#changelog-and-versioning)
12. [Review Checklist](#review-checklist)

---

## General Principles

### Core Values

1. **Clarity over Brevity** - Write for understanding, not to save characters
2. **Audience Awareness** - Document for embedded Rust developers familiar with networking
3. **Maintainability** - Keep documentation close to code and update together
4. **Consistency** - Follow established patterns throughout the codebase
5. **Accuracy** - Verify all technical claims, especially register values and bit positions

### Documentation Hierarchy

```
README.md           → Project overview, quick start, links to other docs
DESIGN.md           → Architecture decisions, system design
TESTING.md          → Test strategy, coverage, running tests
DOCUMENTATION_STANDARDS.md → This document
AGENTS.md           → AI agent guidance
src/lib.rs          → Crate-level documentation
src/<module>/mod.rs → Module-level documentation
src/<file>.rs       → Item-level documentation
```

---

## Rust Doc Comments

### When to Use `///` vs `//!`

```rust
//! Module-level documentation (at top of file)
//! Describes the purpose of the entire module

/// Item-level documentation (before structs, functions, etc.)
/// Describes a specific item
pub fn example() {}
```

### Required Documentation

| Item Type | Required | Notes |
|-----------|----------|-------|
| Public modules | ✅ Yes | Use `//!` at top of `mod.rs` |
| Public structs | ✅ Yes | Describe purpose and invariants |
| Public enums | ✅ Yes | Document each variant |
| Public functions | ✅ Yes | Include parameters, returns, errors |
| Public traits | ✅ Yes | Document expected behavior |
| Public constants | ✅ Yes | Explain purpose and valid values |
| Private items | 🔶 Recommended | Document complex logic |
| Test functions | ❌ Optional | Test name should be descriptive |

### Doc Comment Structure

```rust
/// Short one-line summary ending with a period.
///
/// Longer description if needed. This paragraph provides additional
/// context, explains edge cases, or describes the algorithm used.
///
/// # Arguments
///
/// * `param1` - Description of parameter (for functions with non-obvious params)
/// * `param2` - Description of parameter
///
/// # Returns
///
/// Description of the return value (for non-obvious returns)
///
/// # Errors
///
/// * `ErrorType::Variant` - When this error occurs
///
/// # Panics
///
/// Describe conditions under which this function panics (if any)
///
/// # Safety
///
/// For `unsafe` functions, explain requirements for safe usage
///
/// # Examples
///
/// ```ignore
/// let result = function_name(arg1, arg2);
/// assert!(result.is_ok());
/// ```
pub fn function_name(param1: Type1, param2: Type2) -> Result<Output, Error> {
    // ...
}
```

### Section Order

Use this order when multiple sections are present:

1. Summary line
2. Extended description
3. `# Arguments`
4. `# Returns`
5. `# Errors`
6. `# Panics`
7. `# Safety`
8. `# Examples`
9. `# See Also` (optional)
10. `# Implementation Notes` (optional)

---

## Code Comments

### Inline Comments

```rust
// GOOD: Explains WHY, not WHAT
// Clear the OWN bit before modifying descriptor to prevent race with DMA
desc.clear_owned();

// BAD: Restates the code
// Clear the owned flag
desc.clear_owned();
```

### Block Comments for Complex Logic

```rust
// =============================================================================
// Section Header (use for major logical sections)
// =============================================================================

// -------------------------------------------------------------------------
// Subsection Header (use for minor groupings)
// -------------------------------------------------------------------------
```

### TODO/FIXME/HACK Comments

```rust
// TODO: Brief description of what needs to be done
// TODO(username): Assigned TODO with owner

// FIXME: Description of known bug or issue that needs fixing

// HACK: Explanation of why this workaround exists and when it can be removed

// NOTE: Important information for future readers

// SAFETY: Justification for unsafe code (required for all unsafe blocks)
```

### Hardware Register Comments

```rust
/// Bit 15: Reset the controller (self-clearing)
/// Bits 14-13: Reserved (read as 0)
/// Bits 12-10: Clock divider selection
/// Bit 9: Enable interrupt
pub const CONTROL_REG_OFFSET: usize = 0x04;

// When modifying register values, explain the bit manipulation:
let value = (divider << 10)  // Set clock divider in bits 12:10
          | (1 << 9);        // Enable interrupt (bit 9)
```

---

## Markdown Files

### File Structure

Every markdown file should have:

1. **Title** (H1) - Single `#` heading at the top
2. **Brief description** - One paragraph explaining the document's purpose
3. **Horizontal rule** - `---` after the intro
4. **Table of Contents** - For documents longer than 3 sections
5. **Content sections** - Organized with H2 (`##`) and H3 (`###`)
6. **Horizontal rules** - Between major sections

### Heading Hierarchy

```markdown
# Document Title (only one per file)

## Major Section

### Subsection

#### Minor Subsection (use sparingly)
```

### Tables

Use tables for structured data:

```markdown
| Column 1 | Column 2 | Column 3 |
|----------|----------|----------|
| Value 1  | Value 2  | Value 3  |
| Value 4  | Value 5  | Value 6  |
```

Alignment:
- Left-align text columns (default)
- Right-align numeric columns
- Center-align status/icon columns

### Code Blocks

Always specify the language:

````markdown
```rust
fn example() {}
```

```bash
cargo build --release
```

```text
Plain text output
```
````

### Links

```markdown
<!-- Internal links (relative) -->
See [TESTING.md](TESTING.md) for test documentation.

<!-- Section links -->
See the [Error Handling](#error-handling) section.

<!-- External links -->
See the [ESP-IDF documentation](https://docs.espressif.com/).
```

---

## Module Documentation

### Module Header Template

```rust
//! Brief description of the module.
//!
//! This module provides [main functionality]. It is used for [primary use case].
//!
//! # Overview
//!
//! Describe the module's role in the overall architecture.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐
//! │  Component  │────▶│  Component  │
//! └─────────────┘     └─────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use crate::module_name::MainType;
//!
//! let instance = MainType::new();
//! ```
//!
//! # Features
//!
//! - Feature 1
//! - Feature 2
//!
//! # See Also
//!
//! - [`related_module`] - Description
```

### Re-exports Documentation

```rust
// Document re-exports when they're part of the public API
/// DMA descriptor types for TX and RX operations.
pub use descriptor::{RxDescriptor, TxDescriptor};
```

---

## API Documentation

### Structs

```rust
/// A DMA engine for managing Ethernet frame transmission and reception.
///
/// This structure owns the TX and RX descriptor rings and their associated
/// buffers. All memory is statically allocated using const generics.
///
/// # Type Parameters
///
/// * `RX_BUFS` - Number of receive buffers (recommended: 10-20)
/// * `TX_BUFS` - Number of transmit buffers (recommended: 10-20)  
/// * `BUF_SIZE` - Size of each buffer in bytes (minimum: 1522 for standard frames)
///
/// # Invariants
///
/// - Descriptor rings form circular linked lists
/// - Buffers are 4-byte aligned for DMA access
/// - Only one owner (CPU or DMA) may access a descriptor at a time
///
/// # Example
///
/// ```ignore
/// static mut DMA: DmaEngine<10, 10, 1600> = DmaEngine::new();
/// ```
pub struct DmaEngine<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize> {
    // ...
}
```

### Enums

```rust
/// PHY interface mode selection.
///
/// Determines the physical interface between the MAC and PHY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhyInterface {
    /// Reduced Media Independent Interface (most common for ESP32)
    ///
    /// Uses fewer pins than MII and operates at 50 MHz reference clock.
    #[default]
    Rmii,
    
    /// Media Independent Interface (legacy, rarely used)
    ///
    /// Uses more pins but is more widely compatible with older PHYs.
    Mii,
}
```

### Traits

```rust
/// A bus for communicating with PHY devices via MDIO protocol.
///
/// Implementations must handle the MDIO timing requirements and
/// busy-wait for operation completion.
///
/// # Implementation Requirements
///
/// - `read` and `write` must wait for any pending operation to complete
/// - Operations should timeout after ~1ms to prevent deadlock
/// - The bus must be thread-safe if used from interrupts
pub trait MdioBus {
    /// Read a 16-bit value from a PHY register.
    ///
    /// # Arguments
    ///
    /// * `phy_addr` - PHY address (0-31)
    /// * `reg_addr` - Register address (0-31)
    ///
    /// # Errors
    ///
    /// * `IoError::Timeout` - Bus remained busy too long
    fn read(&mut self, phy_addr: u8, reg_addr: u8) -> Result<u16>;
    
    // ...
}
```

---

## Error Documentation

### Error Enum Documentation

```rust
/// Errors that can occur during DMA operations.
///
/// These errors are recoverable - the DMA engine remains in a valid state
/// and can continue operation after handling the error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaError {
    /// No transmit descriptors are available.
    ///
    /// The TX ring is full. Wait for pending transmissions to complete
    /// or increase `TX_BUFS`.
    TxBuffersFull,
    
    /// No receive descriptors have completed frames.
    ///
    /// No frames are available. This is normal when polling; use
    /// interrupts for efficient waiting.
    NoFrameAvailable,
    
    /// Frame exceeds the maximum buffer size.
    ///
    /// The frame is larger than `BUF_SIZE`. Either increase buffer size
    /// or fragment the frame (not currently supported).
    FrameTooLarge,
}
```

### Error Handling Documentation

When a function can return errors, document:
1. Each error variant that can be returned
2. The conditions that cause each error
3. How to handle or recover from the error

---

## Safety Documentation

### Unsafe Functions

```rust
/// Write directly to a hardware register.
///
/// # Safety
///
/// The caller must ensure:
/// - `addr` is a valid, aligned hardware register address
/// - The register is memory-mapped and accessible
/// - No other code is concurrently accessing the same register
/// - Writing to this register is safe for the current hardware state
///
/// # Example
///
/// ```ignore
/// // SAFETY: EMAC_STATUS is a valid register at this address,
/// // and we have exclusive access during initialization.
/// unsafe { write_reg(EMAC_STATUS, 0xFFFF_FFFF) };
/// ```
pub unsafe fn write_reg(addr: usize, value: u32) {
    // ...
}
```

### Unsafe Blocks

```rust
fn setup_descriptor(&self, buffer: *const u8) {
    // SAFETY: We have exclusive access to this descriptor (not owned by DMA),
    // and the buffer pointer was derived from a valid slice.
    unsafe {
        self.buffer_addr.set(buffer as u32);
    }
}
```

### Safety Invariants for Types

```rust
/// A descriptor that may be accessed by DMA hardware.
///
/// # Safety Invariants
///
/// - When `is_owned()` returns `true`, only DMA hardware may access the descriptor
/// - When `is_owned()` returns `false`, only CPU code may access the descriptor
/// - The `buffer_addr` field must always point to valid, properly-aligned memory
/// - Descriptors must be linked in a valid circular chain before DMA starts
pub struct TxDescriptor {
    // ...
}
```

---

## Examples and Code Snippets

### When to Include Examples

| Complexity | Example Required | Notes |
|------------|------------------|-------|
| Trivial (getters/setters) | ❌ No | Signature is self-explanatory |
| Simple (single operation) | 🔶 Optional | Include if non-obvious |
| Moderate (multiple steps) | ✅ Yes | Show complete usage |
| Complex (error handling) | ✅ Yes | Show error handling pattern |

### Example Format

```rust
/// # Examples
///
/// Basic usage:
///
/// ```ignore
/// let phy = Lan8720a::new(0);
/// let mut mdio = /* ... */;
/// 
/// if phy.is_link_up(&mut mdio)? {
///     let status = phy.link_status(&mut mdio)?;
///     println!("Link: {:?}", status);
/// }
/// ```
///
/// With error handling:
///
/// ```ignore
/// match phy.init(&mut mdio, &mut delay) {
///     Ok(()) => println!("PHY initialized"),
///     Err(e) => println!("Init failed: {}", e),
/// }
/// ```
```

### Use `ignore` for Hardware-Dependent Examples

```rust
/// ```ignore
/// // This example requires ESP32 hardware
/// let dma = DmaEngine::new();
/// dma.init()?;
/// ```
```

---

## Tables and Diagrams

### Register Layout Tables

```rust
/// DMA Bus Mode Register (DMABUSMODE)
///
/// | Bits  | Field | Access | Description |
/// |-------|-------|--------|-------------|
/// | 31:26 | Reserved | RO | Read as 0 |
/// | 25 | MB | RW | Mixed Burst |
/// | 24 | AAL | RW | Address-Aligned Beats |
/// | 23:17 | PBLx8 | RW | PBL multiplied by 8 |
/// | 16 | USP | RW | Use Separate PBL |
/// | 15:14 | RPBL | RW | RX DMA PBL |
/// | 13 | FB | RW | Fixed Burst |
/// | 12:8 | PR | RW | Priority Ratio |
/// | 7 | PM | RW | Priority Mode |
/// | 6:2 | PBL | RW | Programmable Burst Length |
/// | 1 | DA | RW | DMA Arbitration |
/// | 0 | SWR | RW/SC | Software Reset |
```

### ASCII Diagrams

```rust
//! # Data Flow
//!
//! ```text
//! ┌─────────┐    ┌─────────────┐    ┌─────────┐
//! │   CPU   │───▶│ TX Ring     │───▶│   DMA   │
//! │         │    │ [D0][D1]... │    │         │
//! └─────────┘    └─────────────┘    └────┬────┘
//!                                        │
//!                                        ▼
//!                                   ┌─────────┐
//!                                   │   MAC   │
//!                                   └────┬────┘
//!                                        │
//!                                        ▼
//!                                   ┌─────────┐
//!                                   │   PHY   │
//!                                   └─────────┘
//! ```
```

---

## Changelog and Versioning

### Semantic Versioning

Follow [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR**: Breaking API changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

### Documenting Changes

When making significant changes:

1. Update the relevant documentation files
2. Update doc comments for modified APIs
3. Note breaking changes prominently
4. Update examples if API changed

---

## Review Checklist

### Before Submitting Code

- [ ] All public items have doc comments
- [ ] Doc comments have a summary line
- [ ] `# Safety` section for all `unsafe` code
- [ ] `# Errors` section for fallible functions
- [ ] Examples for complex APIs
- [ ] No broken intra-doc links
- [ ] Spell-check documentation

### Documentation Quality Check

```bash
# Check for documentation warnings
cargo doc --no-deps 2>&1 | grep -i warning

# Build docs and view
cargo doc --no-deps --open

# Check doc tests (if any are runnable)
cargo test --doc
```

### Style Verification

- [ ] Consistent heading levels
- [ ] Tables properly formatted
- [ ] Code blocks have language specified
- [ ] Links are relative where possible
- [ ] No trailing whitespace
- [ ] Consistent terminology throughout

---

## Quick Reference

### Common Documentation Patterns

```rust
// Simple getter - minimal docs
/// Returns the current buffer size.
pub fn buffer_size(&self) -> usize { ... }

// Configuration function - document parameters
/// Configure the PHY interface mode.
///
/// # Arguments
///
/// * `mode` - The interface mode ([`PhyInterface::Rmii`] recommended)
pub fn set_interface(&mut self, mode: PhyInterface) { ... }

// Fallible operation - document errors
/// Transmit a frame.
///
/// # Errors
///
/// * [`DmaError::TxBuffersFull`] - No descriptors available
/// * [`DmaError::FrameTooLarge`] - Frame exceeds buffer size
pub fn transmit(&mut self, data: &[u8]) -> Result<(), DmaError> { ... }

// Unsafe operation - document safety
/// # Safety
///
/// Caller must ensure exclusive access to the register.
pub unsafe fn write_raw(&self, value: u32) { ... }
```

### Terminology Glossary

Use consistent terminology throughout:

| Term | Definition |
|------|------------|
| Frame | An Ethernet frame (L2 PDU) |
| Packet | An IP packet (L3 PDU) |
| Descriptor | A DMA descriptor structure |
| Buffer | Memory region for frame data |
| Ring | Circular descriptor queue |
| PHY | Physical layer device |
| MAC | Media Access Controller |
| MDIO | Management Data I/O interface |
| RMII | Reduced Media Independent Interface |
