//! Memory-mapped register definitions for ESP32 EMAC
//!
//! This module provides type-safe access to the EMAC peripheral registers.
//! All register access is volatile to ensure proper hardware interaction.

pub mod dma;
pub mod ext;
pub mod mac;

// ESP32 and ESP32-P4 are mutually exclusive; if both are enabled, prefer ESP32
// If neither is enabled, default to ESP32 addresses

/// DMA register block base address
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const DMA_BASE: usize = 0x3FF6_9000;

/// MAC register block base address
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const MAC_BASE: usize = 0x3FF6_A000;

/// Extension register block base address
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const EXT_BASE: usize = 0x3FF6_9800;

/// DPORT WiFi clock enable register (contains EMAC clock bit)
/// Note: DPORT base is 0x3FF00000, WIFI_CLK_EN offset is 0x0CC
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const DPORT_WIFI_CLK_EN_REG: usize = 0x3FF0_00CC;

/// EMAC clock enable bit in DPORT_WIFI_CLK_EN_REG
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const DPORT_WIFI_CLK_EMAC_EN: u32 = 1 << 14;

// =============================================================================
// IO_MUX Register Definitions (ESP32)
// =============================================================================

/// IO_MUX base address for ESP32
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const IO_MUX_BASE: usize = 0x3FF4_9000;

/// IO_MUX GPIO0 register offset from IO_MUX_BASE
/// Note: GPIO0 is at offset 0x44 in the IO_MUX register block
#[cfg(any(feature = "esp32", not(feature = "esp32p4")))]
pub const IO_MUX_GPIO0_OFFSET: usize = 0x44;

/// IO_MUX FUN_IE bit (input enable) - bit 9
pub const IO_MUX_FUN_IE: u32 = 1 << 9;

/// IO_MUX MCU_SEL field shift - bits 12:14
pub const IO_MUX_MCU_SEL_SHIFT: u32 = 12;

/// IO_MUX MCU_SEL field mask (3 bits)
pub const IO_MUX_MCU_SEL_MASK: u32 = 0x7 << 12;

/// EMAC_TX_CLK function for GPIO0 (used as RMII reference clock input)
/// This is function 5 on ESP32 GPIO0
pub const IO_MUX_GPIO0_FUNC_EMAC_TX_CLK: u32 = 5;

/// DMA register block base address (ESP32-P4)
#[cfg(all(feature = "esp32p4", not(feature = "esp32")))]
pub const DMA_BASE: usize = 0x5008_4000;

/// MAC register block base address (ESP32-P4)
#[cfg(all(feature = "esp32p4", not(feature = "esp32")))]
pub const MAC_BASE: usize = 0x5008_5000;

/// Extension register block base address (ESP32-P4)
#[cfg(all(feature = "esp32p4", not(feature = "esp32")))]
pub const EXT_BASE: usize = 0x5008_4800;

/// Read a 32-bit register at the given address
///
/// # Safety
/// The caller must ensure the address is valid and properly aligned.
#[inline(always)]
pub unsafe fn read_reg(addr: usize) -> u32 {
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

/// Write a 32-bit value to a register at the given address
///
/// # Safety
/// The caller must ensure the address is valid and properly aligned.
#[inline(always)]
pub unsafe fn write_reg(addr: usize, value: u32) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
}

/// Modify a register using a read-modify-write operation
///
/// # Safety
/// The caller must ensure the address is valid and properly aligned.
#[inline(always)]
pub unsafe fn modify_reg<F>(addr: usize, f: F)
where
    F: FnOnce(u32) -> u32,
{
    // SAFETY: caller guarantees address validity
    let value = unsafe { read_reg(addr) };
    unsafe { write_reg(addr, f(value)) }
}

/// Set bits in a register (read-modify-write)
///
/// # Safety
/// The caller must ensure the address is valid and properly aligned.
#[inline(always)]
pub unsafe fn set_bits(addr: usize, bits: u32) {
    // SAFETY: caller guarantees address validity
    unsafe { modify_reg(addr, |v| v | bits) }
}

/// Clear bits in a register (read-modify-write)
///
/// # Safety
/// The caller must ensure the address is valid and properly aligned.
#[inline(always)]
pub unsafe fn clear_bits(addr: usize, bits: u32) {
    // SAFETY: caller guarantees address validity
    unsafe { modify_reg(addr, |v| v & !bits) }
}

// =============================================================================
// Register Access Macros
// =============================================================================

/// Generate read/write accessor methods for a register.
///
/// # Example
/// ```ignore
/// impl DmaRegs {
///     reg_rw!(bus_mode, set_bus_mode, DMA_BASE, DMABUSMODE_OFFSET,
///             "Bus Mode register");
/// }
/// ```
macro_rules! reg_rw {
    ($read_fn:ident, $write_fn:ident, $base:expr, $offset:expr, $doc:expr) => {
        #[doc = concat!("Read ", $doc)]
        #[inline(always)]
        pub fn $read_fn() -> u32 {
            unsafe { $crate::internal::register::read_reg($base + $offset) }
        }

        #[doc = concat!("Write ", $doc)]
        #[inline(always)]
        pub fn $write_fn(value: u32) {
            unsafe { $crate::internal::register::write_reg($base + $offset, value) }
        }
    };
}

/// Generate a read-only accessor method for a register.
macro_rules! reg_ro {
    ($read_fn:ident, $base:expr, $offset:expr, $doc:expr) => {
        #[doc = concat!("Read ", $doc)]
        #[inline(always)]
        pub fn $read_fn() -> u32 {
            unsafe { $crate::internal::register::read_reg($base + $offset) }
        }
    };
}

/// Generate set/clear bit operation methods for a register.
///
/// # Example
/// ```ignore
/// impl DmaRegs {
///     reg_bit_ops!(start_tx, stop_tx, DMA_BASE, DMAOPERATION_OFFSET, DMAOPERATION_ST,
///                  "TX DMA", "Start", "Stop");
/// }
/// ```
macro_rules! reg_bit_ops {
    ($set_fn:ident, $clear_fn:ident, $base:expr, $offset:expr, $bit:expr, $what:expr, $set_verb:expr, $clear_verb:expr) => {
        #[doc = concat!($set_verb, " ", $what)]
        #[inline(always)]
        pub fn $set_fn() {
            unsafe { $crate::internal::register::set_bits($base + $offset, $bit) }
        }

        #[doc = concat!($clear_verb, " ", $what)]
        #[inline(always)]
        pub fn $clear_fn() {
            unsafe { $crate::internal::register::clear_bits($base + $offset, $bit) }
        }
    };
}

/// Generate a bit check method (inverted - true when bit is clear).
macro_rules! reg_bit_check_clear {
    ($fn:ident, $base:expr, $offset:expr, $bit:expr, $doc:expr) => {
        #[doc = $doc]
        #[inline(always)]
        pub fn $fn() -> bool {
            unsafe { ($crate::internal::register::read_reg($base + $offset) & $bit) == 0 }
        }
    };
}

// Export macros for use in submodules
pub(crate) use reg_bit_check_clear;
pub(crate) use reg_bit_ops;
pub(crate) use reg_ro;
pub(crate) use reg_rw;
