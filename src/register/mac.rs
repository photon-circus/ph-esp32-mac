//! MAC Core Register Definitions
//!
//! The MAC core handles frame transmission and reception according to IEEE 802.3.

use super::{
    reg_bit_ops, reg_ro, reg_rw,
    read_reg, write_reg, MAC_BASE,
};

// =============================================================================
// Register Offsets
// =============================================================================

/// GMAC Configuration Register offset
pub const GMACCONFIG_OFFSET: usize = 0x00;
/// GMAC Frame Filter Register offset
pub const GMACFF_OFFSET: usize = 0x04;
/// GMAC Hash Table High Register offset
pub const GMACHASTH_OFFSET: usize = 0x08;
/// GMAC Hash Table Low Register offset
pub const GMACHASTL_OFFSET: usize = 0x0C;
/// GMAC MII Address Register offset
pub const GMACMIIADDR_OFFSET: usize = 0x10;
/// GMAC MII Data Register offset
pub const GMACMIIDATA_OFFSET: usize = 0x14;
/// GMAC Flow Control Register offset
pub const GMACFC_OFFSET: usize = 0x18;
/// GMAC VLAN Tag Register offset
pub const GMACVLAN_OFFSET: usize = 0x1C;
/// GMAC Debug Register offset (read-only)
pub const GMACDEBUG_OFFSET: usize = 0x24;
/// GMAC PMT Control and Status Register offset
pub const GMACPMT_OFFSET: usize = 0x2C;
/// GMAC LPI Control and Status Register offset
pub const GMACLPI_OFFSET: usize = 0x30;
/// GMAC LPI Timer Control Register offset
pub const GMACLPITIMER_OFFSET: usize = 0x34;
/// GMAC Interrupt Status Register offset
pub const GMACINTS_OFFSET: usize = 0x38;
/// GMAC Interrupt Mask Register offset
pub const GMACINTMASK_OFFSET: usize = 0x3C;
/// GMAC Address 0 High Register offset (upper 16 bits of MAC address)
pub const GMACADDR0H_OFFSET: usize = 0x40;
/// GMAC Address 0 Low Register offset (lower 32 bits of MAC address)
pub const GMACADDR0L_OFFSET: usize = 0x44;
/// GMAC Address 1 High Register offset (additional filter 1)
pub const GMACADDR1H_OFFSET: usize = 0x48;
/// GMAC Address 1 Low Register offset
pub const GMACADDR1L_OFFSET: usize = 0x4C;
/// GMAC Address 2 High Register offset (additional filter 2)
pub const GMACADDR2H_OFFSET: usize = 0x50;
/// GMAC Address 2 Low Register offset
pub const GMACADDR2L_OFFSET: usize = 0x54;
/// GMAC Address 3 High Register offset (additional filter 3)
pub const GMACADDR3H_OFFSET: usize = 0x58;
/// GMAC Address 3 Low Register offset
pub const GMACADDR3L_OFFSET: usize = 0x5C;
/// GMAC Address 4 High Register offset (additional filter 4) - ESP32 max
pub const GMACADDR4H_OFFSET: usize = 0x60;
/// GMAC Address 4 Low Register offset
pub const GMACADDR4L_OFFSET: usize = 0x64;

/// Number of additional MAC address filter slots (beyond primary)
pub const MAC_ADDR_FILTER_COUNT: usize = 4;

// =============================================================================
// GMAC Address High Register Bits (for filter slots 1-4)
// =============================================================================

/// Address Enable - when set, address comparison is enabled
pub const GMACADDRH_AE: u32 = 1 << 31;
/// Source Address filter - when set, compares SA instead of DA
pub const GMACADDRH_SA: u32 = 1 << 30;
/// Mask Byte Control shift (bits 29:24)
pub const GMACADDRH_MBC_SHIFT: u32 = 24;
/// Mask Byte Control mask - each bit masks one byte of address comparison
pub const GMACADDRH_MBC_MASK: u32 = 0x3F << 24;

/// GMAC AN Control Register offset
pub const GMACAN_OFFSET: usize = 0xC0;
/// GMAC AN Status Register offset
pub const GMACANS_OFFSET: usize = 0xC4;
/// GMAC AN Advertisement Register offset
pub const GMACANA_OFFSET: usize = 0xC8;
/// GMAC AN Link Partner Ability Register offset
pub const GMACANLPA_OFFSET: usize = 0xCC;
/// GMAC AN Expansion Register offset
pub const GMACANE_OFFSET: usize = 0xD0;
/// GMAC TBI Extended Status Register offset
pub const GMACTBI_OFFSET: usize = 0xD4;
/// GMAC SGMII/RGMII Status Register offset
pub const GMACSGMII_OFFSET: usize = 0xD8;

// =============================================================================
// GMAC Configuration Register (GMACCONFIG) Bits
// =============================================================================

/// Preamble Length for Transmit shift
pub const GMACCONFIG_PRELEN_SHIFT: u32 = 0;
/// Preamble Length mask (2 bits)
pub const GMACCONFIG_PRELEN_MASK: u32 = 0x3;
/// Receiver Enable
pub const GMACCONFIG_RE: u32 = 1 << 2;
/// Transmitter Enable
pub const GMACCONFIG_TE: u32 = 1 << 3;
/// Deferral Check (half-duplex only)
pub const GMACCONFIG_DC: u32 = 1 << 4;
/// Back-Off Limit shift
pub const GMACCONFIG_BL_SHIFT: u32 = 5;
/// Back-Off Limit mask (2 bits)
pub const GMACCONFIG_BL_MASK: u32 = 0x3 << 5;
/// Automatic Pad/CRC Stripping
pub const GMACCONFIG_ACS: u32 = 1 << 7;
/// Link Up/Down (ESP32-specific)
pub const GMACCONFIG_LUD: u32 = 1 << 8;
/// Retry Disable
pub const GMACCONFIG_DR: u32 = 1 << 9;
/// Checksum Offload (IPC)
pub const GMACCONFIG_IPC: u32 = 1 << 10;
/// Duplex Mode: 0 = half, 1 = full
pub const GMACCONFIG_DM: u32 = 1 << 11;
/// Loopback Mode
pub const GMACCONFIG_LM: u32 = 1 << 12;
/// Disable Receive Own (half-duplex only)
pub const GMACCONFIG_DO: u32 = 1 << 13;
/// Speed: 0 = 10 Mbps, 1 = 100 Mbps
pub const GMACCONFIG_FES: u32 = 1 << 14;
/// Port Select: must be 1 for MII/RMII
pub const GMACCONFIG_PS: u32 = 1 << 15;
/// Disable Carrier Sense During Transmission
pub const GMACCONFIG_DCRS: u32 = 1 << 16;
/// Inter-Frame Gap shift
pub const GMACCONFIG_IFG_SHIFT: u32 = 17;
/// Inter-Frame Gap mask (3 bits)
pub const GMACCONFIG_IFG_MASK: u32 = 0x7 << 17;
/// Jumbo Frame Enable
pub const GMACCONFIG_JE: u32 = 1 << 20;
/// Frame Burst Enable (reserved, keep 0)
pub const GMACCONFIG_BE: u32 = 1 << 21;
/// Jabber Disable
pub const GMACCONFIG_JD: u32 = 1 << 22;
/// Watchdog Disable
pub const GMACCONFIG_WD: u32 = 1 << 23;
/// Transmit Configuration in RGMII/SGMII
pub const GMACCONFIG_TC: u32 = 1 << 24;
/// CRC Stripping for Type Frames
pub const GMACCONFIG_CST: u32 = 1 << 25;
/// SGMII/RGMII Mode Enable
pub const GMACCONFIG_SFTERR: u32 = 1 << 26;
/// 2K Packets Enable
pub const GMACCONFIG_TWOKPE: u32 = 1 << 27;
/// Source Address Insertion/Replacement Control shift
pub const GMACCONFIG_SARC_SHIFT: u32 = 28;
/// Source Address Insertion/Replacement Control mask
pub const GMACCONFIG_SARC_MASK: u32 = 0x7 << 28;

/// Inter-Frame Gap values (in bit times / 8)
pub mod ifg {
    /// 96 bit times (default)
    pub const IFG_96: u32 = 0;
    /// 88 bit times
    pub const IFG_88: u32 = 1;
    /// 80 bit times
    pub const IFG_80: u32 = 2;
    /// 72 bit times
    pub const IFG_72: u32 = 3;
    /// 64 bit times
    pub const IFG_64: u32 = 4;
    /// 56 bit times
    pub const IFG_56: u32 = 5;
    /// 48 bit times
    pub const IFG_48: u32 = 6;
    /// 40 bit times (minimum)
    pub const IFG_40: u32 = 7;
}

// =============================================================================
// GMAC Frame Filter Register (GMACFF) Bits
// =============================================================================

/// Promiscuous Mode
pub const GMACFF_PR: u32 = 1 << 0;
/// Hash Unicast
pub const GMACFF_HUC: u32 = 1 << 1;
/// Hash Multicast
pub const GMACFF_HMC: u32 = 1 << 2;
/// DA Inverse Filtering
pub const GMACFF_DAIF: u32 = 1 << 3;
/// Pass All Multicast
pub const GMACFF_PM: u32 = 1 << 4;
/// Disable Broadcast Frames
pub const GMACFF_DBF: u32 = 1 << 5;
/// Pass Control Frames shift
pub const GMACFF_PCF_SHIFT: u32 = 6;
/// Pass Control Frames mask
pub const GMACFF_PCF_MASK: u32 = 0x3 << 6;
/// SA Inverse Filtering
pub const GMACFF_SAIF: u32 = 1 << 8;
/// Source Address Filter Enable
pub const GMACFF_SAF: u32 = 1 << 9;
/// Hash or Perfect Filter
pub const GMACFF_HPF: u32 = 1 << 10;
/// VLAN Tag Filter Enable
pub const GMACFF_VTFE: u32 = 1 << 16;
/// Layer 3/4 Filter Enable
pub const GMACFF_IPFE: u32 = 1 << 20;
/// Drop Non-TCP/UDP over IP Frames
pub const GMACFF_DNTU: u32 = 1 << 21;
/// Receive All
pub const GMACFF_RA: u32 = 1 << 31;

/// Pass control frames modes
pub mod pcf {
    /// Do not pass control frames
    pub const NONE: u32 = 0;
    /// Pass all control frames except PAUSE
    pub const ALL_EXCEPT_PAUSE: u32 = 1;
    /// Pass all control frames
    pub const ALL: u32 = 2;
    /// Pass control frames that pass address filter
    pub const FILTERED: u32 = 3;
}

// =============================================================================
// GMAC VLAN Tag Register (GMACVLAN) Bits
// =============================================================================

/// VLAN Tag Identifier (VID) mask (bits 15:0)
pub const GMACVLAN_VL_MASK: u32 = 0xFFFF;
/// Enable 12-bit VLAN Tag Comparison (only VID, not priority)
/// When set: compare only bits 11:0 of VLAN tag
/// When clear: compare all 16 bits (priority + CFI + VID)
pub const GMACVLAN_ETV: u32 = 1 << 16;
/// VLAN Tag Inverse Match
/// When set: frames NOT matching VLAN tag are passed
pub const GMACVLAN_VTIM: u32 = 1 << 17;
/// Enable S-VLAN (0x88A8) instead of C-VLAN (0x8100)
pub const GMACVLAN_ESVL: u32 = 1 << 18;
/// VLAN Tag Hash Table Match Enable
pub const GMACVLAN_VTHM: u32 = 1 << 19;

// =============================================================================
// GMAC MII Address Register (GMACMIIADDR) Bits
// =============================================================================

/// MII Busy
pub const GMACMIIADDR_GB: u32 = 1 << 0;
/// MII Write
pub const GMACMIIADDR_GW: u32 = 1 << 1;
/// CSR Clock Range shift
pub const GMACMIIADDR_CR_SHIFT: u32 = 2;
/// CSR Clock Range mask (4 bits)
pub const GMACMIIADDR_CR_MASK: u32 = 0xF << 2;
/// MII Register Address shift
pub const GMACMIIADDR_GR_SHIFT: u32 = 6;
/// MII Register Address mask (5 bits)
pub const GMACMIIADDR_GR_MASK: u32 = 0x1F << 6;
/// Physical Layer Address shift
pub const GMACMIIADDR_PA_SHIFT: u32 = 11;
/// Physical Layer Address mask (5 bits)
pub const GMACMIIADDR_PA_MASK: u32 = 0x1F << 11;

/// CSR clock divider values for MDC clock generation
pub mod csr_clock {
    /// CSR clock / 42 (60-100 MHz)
    pub const DIV_42: u32 = 0;
    /// CSR clock / 62 (100-150 MHz)
    pub const DIV_62: u32 = 1;
    /// CSR clock / 16 (20-35 MHz)
    pub const DIV_16: u32 = 2;
    /// CSR clock / 26 (35-60 MHz)
    pub const DIV_26: u32 = 3;
    /// CSR clock / 102 (150-250 MHz)
    pub const DIV_102: u32 = 4;
    /// CSR clock / 124 (250-300 MHz)
    pub const DIV_124: u32 = 5;
}

// =============================================================================
// GMAC Flow Control Register (GMACFC) Bits
// =============================================================================

/// Flow Control Busy/Backpressure Activate
pub const GMACFC_FCB_BPA: u32 = 1 << 0;
/// Transmit Flow Control Enable
pub const GMACFC_TFE: u32 = 1 << 1;
/// Receive Flow Control Enable
pub const GMACFC_RFE: u32 = 1 << 2;
/// Unicast PAUSE Frame Detect
pub const GMACFC_UP: u32 = 1 << 3;
/// PAUSE Low Threshold shift
pub const GMACFC_PLT_SHIFT: u32 = 4;
/// PAUSE Low Threshold mask
pub const GMACFC_PLT_MASK: u32 = 0x3 << 4;
/// Zero-Quanta PAUSE Disable
pub const GMACFC_DZPQ: u32 = 1 << 7;
/// PAUSE Time shift
pub const GMACFC_PT_SHIFT: u32 = 16;
/// PAUSE Time mask
pub const GMACFC_PT_MASK: u32 = 0xFFFF << 16;

// =============================================================================
// GMAC Debug Register (GMACDEBUG) Bits (Read-Only)
// =============================================================================

/// GMAC RX FIFO not empty
pub const GMACDEBUG_RXFNE: u32 = 1 << 0;
/// GMAC RX Controller state shift
pub const GMACDEBUG_RXFC_SHIFT: u32 = 1;
/// GMAC RX Controller state mask
pub const GMACDEBUG_RXFC_MASK: u32 = 0x3 << 1;
/// GMAC RX FIFO read controller active
pub const GMACDEBUG_RXFRCA: u32 = 1 << 4;
/// GMAC RX FIFO write controller active
pub const GMACDEBUG_RXFWCA: u32 = 1 << 5;
/// GMAC RX FIFO state shift
pub const GMACDEBUG_RXFFS_SHIFT: u32 = 8;
/// GMAC RX FIFO state mask
pub const GMACDEBUG_RXFFS_MASK: u32 = 0x3 << 8;
/// GMAC TX FIFO not empty
pub const GMACDEBUG_TXFNE: u32 = 1 << 16;
/// GMAC TX FIFO write active
pub const GMACDEBUG_TXFWA: u32 = 1 << 17;
/// GMAC TX FIFO read active
pub const GMACDEBUG_TXFRA: u32 = 1 << 20;
/// GMAC TX Controller state shift
pub const GMACDEBUG_TXFC_SHIFT: u32 = 21;
/// GMAC TX Controller state mask
pub const GMACDEBUG_TXFC_MASK: u32 = 0x3 << 21;
/// GMAC TX FIFO not full
pub const GMACDEBUG_TXFNF: u32 = 1 << 24;
/// GMAC TX FIFO full
pub const GMACDEBUG_TXFF: u32 = 1 << 25;

// =============================================================================
// MAC Register Access Functions
// =============================================================================

/// MAC Register block for type-safe access
pub struct MacRegs;

impl MacRegs {
    /// Get the base address
    #[inline(always)]
    pub const fn base() -> usize {
        MAC_BASE
    }

    // -------------------------------------------------------------------------
    // Register accessors (generated by macros)
    // -------------------------------------------------------------------------

    reg_rw!(config, set_config, MAC_BASE, GMACCONFIG_OFFSET, "GMAC Configuration register");
    reg_rw!(frame_filter, set_frame_filter, MAC_BASE, GMACFF_OFFSET, "Frame Filter register");
    reg_rw!(hash_table_high, set_hash_table_high, MAC_BASE, GMACHASTH_OFFSET, "Hash Table High register");
    reg_rw!(hash_table_low, set_hash_table_low, MAC_BASE, GMACHASTL_OFFSET, "Hash Table Low register");
    reg_rw!(mii_address, set_mii_address, MAC_BASE, GMACMIIADDR_OFFSET, "MII Address register");
    reg_rw!(mii_data, set_mii_data, MAC_BASE, GMACMIIDATA_OFFSET, "MII Data register");
    reg_rw!(flow_control, set_flow_control, MAC_BASE, GMACFC_OFFSET, "Flow Control register");
    reg_rw!(vlan_tag, set_vlan_tag, MAC_BASE, GMACVLAN_OFFSET, "VLAN Tag register");
    reg_rw!(interrupt_mask, set_interrupt_mask, MAC_BASE, GMACINTMASK_OFFSET, "Interrupt Mask register");
    reg_rw!(mac_addr0_high, set_mac_addr0_high, MAC_BASE, GMACADDR0H_OFFSET, "MAC Address 0 High register");
    reg_rw!(mac_addr0_low, set_mac_addr0_low, MAC_BASE, GMACADDR0L_OFFSET, "MAC Address 0 Low register");

    reg_ro!(debug, MAC_BASE, GMACDEBUG_OFFSET, "Debug register");
    reg_ro!(interrupt_status, MAC_BASE, GMACINTS_OFFSET, "Interrupt Status register");

    // -------------------------------------------------------------------------
    // Bit operations (generated by macros)
    // -------------------------------------------------------------------------

    reg_bit_ops!(enable_tx, disable_tx, MAC_BASE, GMACCONFIG_OFFSET, GMACCONFIG_TE, "transmitter", "Enable", "Disable");
    reg_bit_ops!(enable_rx, disable_rx, MAC_BASE, GMACCONFIG_OFFSET, GMACCONFIG_RE, "receiver", "Enable", "Disable");

    // -------------------------------------------------------------------------
    // Configuration helpers (conditional bit operations)
    // -------------------------------------------------------------------------

    /// Set duplex mode
    #[inline(always)]
    pub fn set_duplex_full(full: bool) {
        unsafe {
            let cfg = read_reg(MAC_BASE + GMACCONFIG_OFFSET);
            let cfg = if full {
                cfg | GMACCONFIG_DM
            } else {
                cfg & !GMACCONFIG_DM
            };
            write_reg(MAC_BASE + GMACCONFIG_OFFSET, cfg);
        }
    }

    /// Set speed to 100 Mbps
    #[inline(always)]
    pub fn set_speed_100mbps(is_100: bool) {
        unsafe {
            let cfg = read_reg(MAC_BASE + GMACCONFIG_OFFSET);
            let cfg = if is_100 {
                cfg | GMACCONFIG_FES
            } else {
                cfg & !GMACCONFIG_FES
            };
            write_reg(MAC_BASE + GMACCONFIG_OFFSET, cfg);
        }
    }

    /// Enable checksum offload
    #[inline(always)]
    pub fn set_checksum_offload(enable: bool) {
        unsafe {
            let cfg = read_reg(MAC_BASE + GMACCONFIG_OFFSET);
            let cfg = if enable {
                cfg | GMACCONFIG_IPC
            } else {
                cfg & !GMACCONFIG_IPC
            };
            write_reg(MAC_BASE + GMACCONFIG_OFFSET, cfg);
        }
    }

    /// Enable promiscuous mode
    #[inline(always)]
    pub fn set_promiscuous(enable: bool) {
        unsafe {
            let ff = read_reg(MAC_BASE + GMACFF_OFFSET);
            let ff = if enable { ff | GMACFF_PR } else { ff & !GMACFF_PR };
            write_reg(MAC_BASE + GMACFF_OFFSET, ff);
        }
    }

    // -------------------------------------------------------------------------
    // Hash table operations
    // -------------------------------------------------------------------------

    /// Get full 64-bit hash table
    #[inline(always)]
    pub fn hash_table() -> u64 {
        let low = Self::hash_table_low() as u64;
        let high = Self::hash_table_high() as u64;
        low | (high << 32)
    }

    /// Set full 64-bit hash table
    #[inline(always)]
    pub fn set_hash_table(value: u64) {
        Self::set_hash_table_low(value as u32);
        Self::set_hash_table_high((value >> 32) as u32);
    }

    /// Clear entire hash table
    #[inline(always)]
    pub fn clear_hash_table() {
        Self::set_hash_table_low(0);
        Self::set_hash_table_high(0);
    }

    /// Compute hash index for a MAC address
    ///
    /// Uses the Ethernet CRC-32 polynomial to compute a 6-bit hash index.
    /// The MAC hardware uses the upper 6 bits of the CRC-32 as the hash.
    ///
    /// # Arguments
    /// * `addr` - 6-byte MAC address
    ///
    /// # Returns
    /// A value 0-63 representing the bit position in the 64-bit hash table
    pub fn compute_hash_index(addr: &[u8; 6]) -> u8 {
        const CRC32_POLY: u32 = 0xEDB8_8320;
        let mut crc: u32 = 0xFFFF_FFFF;

        for byte in addr {
            let mut data = *byte;
            for _ in 0..8 {
                if ((crc ^ data as u32) & 1) != 0 {
                    crc = (crc >> 1) ^ CRC32_POLY;
                } else {
                    crc >>= 1;
                }
                data >>= 1;
            }
        }
        (crc & 0x3F) as u8
    }

    /// Set a bit in the hash table
    pub fn set_hash_bit(index: u8) {
        let index = index & 0x3F;
        if index < 32 {
            let current = Self::hash_table_low();
            Self::set_hash_table_low(current | (1 << index));
        } else {
            let current = Self::hash_table_high();
            Self::set_hash_table_high(current | (1 << (index - 32)));
        }
    }

    /// Clear a bit in the hash table
    pub fn clear_hash_bit(index: u8) {
        let index = index & 0x3F;
        if index < 32 {
            let current = Self::hash_table_low();
            Self::set_hash_table_low(current & !(1 << index));
        } else {
            let current = Self::hash_table_high();
            Self::set_hash_table_high(current & !(1 << (index - 32)));
        }
    }

    /// Check if a bit is set in the hash table
    pub fn is_hash_bit_set(index: u8) -> bool {
        let index = index & 0x3F;
        if index < 32 {
            (Self::hash_table_low() & (1 << index)) != 0
        } else {
            (Self::hash_table_high() & (1 << (index - 32))) != 0
        }
    }

    /// Enable hash unicast filtering
    pub fn enable_hash_unicast(enable: bool) {
        unsafe {
            let ff = read_reg(MAC_BASE + GMACFF_OFFSET);
            let ff = if enable { ff | GMACFF_HUC } else { ff & !GMACFF_HUC };
            write_reg(MAC_BASE + GMACFF_OFFSET, ff);
        }
    }

    /// Enable hash multicast filtering
    ///
    /// When enabled, multicast frames are filtered using the hash table.
    /// This is more efficient than "pass all multicast" for subscribing
    /// to specific multicast groups.
    pub fn enable_hash_multicast(enable: bool) {
        unsafe {
            let ff = read_reg(MAC_BASE + GMACFF_OFFSET);
            let ff = if enable {
                ff | GMACFF_HMC
            } else {
                ff & !GMACFF_HMC
            };
            write_reg(MAC_BASE + GMACFF_OFFSET, ff);
        }
    }

    /// Set Hash or Perfect filter mode
    ///
    /// When enabled (HPF=1): Perfect filter for unicast, hash for multicast
    /// When disabled (HPF=0): Hash filter OR perfect filter passes frame
    pub fn set_hash_perfect_filter(enable: bool) {
        unsafe {
            let ff = read_reg(MAC_BASE + GMACFF_OFFSET);
            let ff = if enable {
                ff | GMACFF_HPF
            } else {
                ff & !GMACFF_HPF
            };
            write_reg(MAC_BASE + GMACFF_OFFSET, ff);
        }
    }

    // =========================================================================
    // VLAN Tag Filtering
    // =========================================================================

    /// Enable VLAN tag filtering
    ///
    /// When enabled, frames are filtered based on the VLAN tag.
    pub fn enable_vlan_filter(enable: bool) {
        unsafe {
            let ff = read_reg(MAC_BASE + GMACFF_OFFSET);
            let ff = if enable {
                ff | GMACFF_VTFE
            } else {
                ff & !GMACFF_VTFE
            };
            write_reg(MAC_BASE + GMACFF_OFFSET, ff);
        }
    }

    /// Configure VLAN tag filter
    ///
    /// # Arguments
    /// * `vid` - VLAN Identifier (0-4095) or full 16-bit tag if `vid_only` is false
    /// * `vid_only` - If true, only compare 12-bit VID; if false, compare full 16-bit tag
    /// * `inverse` - If true, pass frames that DON'T match the VLAN tag
    /// * `svlan` - If true, match S-VLAN (0x88A8); if false, match C-VLAN (0x8100)
    pub fn configure_vlan_filter(vid: u16, vid_only: bool, inverse: bool, svlan: bool) {
        let mut vlan = (vid as u32) & GMACVLAN_VL_MASK;

        if vid_only {
            vlan |= GMACVLAN_ETV;
        }
        if inverse {
            vlan |= GMACVLAN_VTIM;
        }
        if svlan {
            vlan |= GMACVLAN_ESVL;
        }

        Self::set_vlan_tag(vlan);
    }

    /// Set VLAN ID filter (simple helper for common case)
    ///
    /// Configures the filter to match frames with the specified VLAN ID,
    /// comparing only the 12-bit VID field.
    ///
    /// # Arguments
    /// * `vid` - VLAN Identifier (0-4095)
    pub fn set_vlan_id_filter(vid: u16) {
        Self::configure_vlan_filter(vid & 0x0FFF, true, false, false);
    }

    /// Get the currently configured VLAN ID filter
    ///
    /// Returns the 12-bit VLAN ID from the VLAN tag register.
    pub fn get_vlan_id_filter() -> u16 {
        (Self::vlan_tag() & 0x0FFF) as u16
    }

    /// Clear VLAN filter (disable and reset)
    pub fn clear_vlan_filter() {
        Self::set_vlan_tag(0);
        Self::enable_vlan_filter(false);
    }

    /// Check if VLAN filtering is enabled
    pub fn is_vlan_filter_enabled() -> bool {
        unsafe { (read_reg(MAC_BASE + GMACFF_OFFSET) & GMACFF_VTFE) != 0 }
    }

    // =========================================================================
    // MII / MDIO Interface
    // =========================================================================

    /// Check if MII is busy
    #[inline(always)]
    pub fn is_mii_busy() -> bool {
        (Self::mii_address() & GMACMIIADDR_GB) != 0
    }

    /// Enable TX flow control (transmit PAUSE frames)
    #[inline(always)]
    pub fn enable_tx_flow_control(enable: bool) {
        unsafe {
            let fc = read_reg(MAC_BASE + GMACFC_OFFSET);
            let fc = if enable {
                fc | GMACFC_TFE
            } else {
                fc & !GMACFC_TFE
            };
            write_reg(MAC_BASE + GMACFC_OFFSET, fc);
        }
    }

    /// Enable RX flow control (respond to PAUSE frames)
    #[inline(always)]
    pub fn enable_rx_flow_control(enable: bool) {
        unsafe {
            let fc = read_reg(MAC_BASE + GMACFC_OFFSET);
            let fc = if enable {
                fc | GMACFC_RFE
            } else {
                fc & !GMACFC_RFE
            };
            write_reg(MAC_BASE + GMACFC_OFFSET, fc);
        }
    }

    /// Configure full flow control settings
    ///
    /// # Arguments
    /// * `pause_time` - PAUSE time in slot times (512 bit times)
    /// * `plt` - PAUSE low threshold (0-3)
    /// * `unicast_detect` - Enable unicast PAUSE frame detection
    /// * `tx_enable` - Enable TX PAUSE frame generation
    /// * `rx_enable` - Enable RX PAUSE frame processing
    pub fn configure_flow_control(
        pause_time: u16,
        plt: u8,
        unicast_detect: bool,
        tx_enable: bool,
        rx_enable: bool,
    ) {
        let mut fc = 0u32;

        // PAUSE time (bits 31:16)
        fc |= (pause_time as u32) << GMACFC_PT_SHIFT;

        // PAUSE low threshold (bits 5:4)
        fc |= ((plt as u32) & 0x3) << GMACFC_PLT_SHIFT;

        // Unicast PAUSE detect
        if unicast_detect {
            fc |= GMACFC_UP;
        }

        // TX flow control enable
        if tx_enable {
            fc |= GMACFC_TFE;
        }

        // RX flow control enable
        if rx_enable {
            fc |= GMACFC_RFE;
        }

        unsafe { write_reg(MAC_BASE + GMACFC_OFFSET, fc) }
    }

    /// Initiate PAUSE frame transmission
    ///
    /// When `activate` is true, sends a PAUSE frame requesting the peer to stop.
    /// When `activate` is false, sends a PAUSE frame with zero quanta to resume.
    ///
    /// In full-duplex mode: FCB (Flow Control Busy) triggers PAUSE TX
    /// In half-duplex mode: BPA (Backpressure Activate) asserts carrier
    pub fn send_pause_frame(activate: bool) {
        unsafe {
            let fc = read_reg(MAC_BASE + GMACFC_OFFSET);
            let fc = if activate {
                fc | GMACFC_FCB_BPA
            } else {
                fc & !GMACFC_FCB_BPA
            };
            write_reg(MAC_BASE + GMACFC_OFFSET, fc);
        }
    }

    /// Check if PAUSE frame transmission is busy
    #[inline(always)]
    pub fn is_flow_control_busy() -> bool {
        (Self::flow_control() & GMACFC_FCB_BPA) != 0
    }

    /// Enable pass all multicast frames
    #[inline(always)]
    pub fn set_pass_all_multicast(enable: bool) {
        unsafe {
            let ff = read_reg(MAC_BASE + GMACFF_OFFSET);
            let ff = if enable { ff | GMACFF_PM } else { ff & !GMACFF_PM };
            write_reg(MAC_BASE + GMACFF_OFFSET, ff);
        }
    }

    /// Set the primary MAC address (6 bytes)
    pub fn set_mac_address(addr: &[u8; 6]) {
        // Low register: addr[0] | (addr[1] << 8) | (addr[2] << 16) | (addr[3] << 24)
        let low = (addr[0] as u32)
            | ((addr[1] as u32) << 8)
            | ((addr[2] as u32) << 16)
            | ((addr[3] as u32) << 24);

        // High register: addr[4] | (addr[5] << 8) | Address Enable (bit 31)
        let high = (addr[4] as u32) | ((addr[5] as u32) << 8) | (1 << 31);

        Self::set_mac_addr0_low(low);
        Self::set_mac_addr0_high(high);
    }

    /// Get the primary MAC address
    pub fn get_mac_address() -> [u8; 6] {
        let low = Self::mac_addr0_low();
        let high = Self::mac_addr0_high();

        [
            (low & 0xFF) as u8,
            ((low >> 8) & 0xFF) as u8,
            ((low >> 16) & 0xFF) as u8,
            ((low >> 24) & 0xFF) as u8,
            (high & 0xFF) as u8,
            ((high >> 8) & 0xFF) as u8,
        ]
    }

    // =========================================================================
    // Additional MAC Address Filters (slots 1-4)
    // =========================================================================

    /// Get the register offset for a MAC address filter slot (1-4)
    ///
    /// Returns (high_offset, low_offset) or None if slot is invalid.
    #[inline(always)]
    const fn addr_filter_offsets(slot: usize) -> Option<(usize, usize)> {
        match slot {
            1 => Some((GMACADDR1H_OFFSET, GMACADDR1L_OFFSET)),
            2 => Some((GMACADDR2H_OFFSET, GMACADDR2L_OFFSET)),
            3 => Some((GMACADDR3H_OFFSET, GMACADDR3L_OFFSET)),
            4 => Some((GMACADDR4H_OFFSET, GMACADDR4L_OFFSET)),
            _ => None,
        }
    }

    /// Set a MAC address filter (slots 1-4)
    ///
    /// # Arguments
    /// * `slot` - Filter slot (1-4)
    /// * `addr` - MAC address to filter
    /// * `source_addr` - If true, filter by source address; if false, by destination
    /// * `mask` - Byte mask (each bit masks one byte, bit 0 = addr[0])
    ///
    /// # Returns
    /// `true` if successful, `false` if slot is invalid
    pub fn set_mac_filter(slot: usize, addr: &[u8; 6], source_addr: bool, mask: u8) -> bool {
        let Some((high_off, low_off)) = Self::addr_filter_offsets(slot) else {
            return false;
        };

        // Low register: addr[0] | (addr[1] << 8) | (addr[2] << 16) | (addr[3] << 24)
        let low = (addr[0] as u32)
            | ((addr[1] as u32) << 8)
            | ((addr[2] as u32) << 16)
            | ((addr[3] as u32) << 24);

        // High register: addr[4] | (addr[5] << 8) | MBC | SA | AE
        let mut high = (addr[4] as u32) | ((addr[5] as u32) << 8);

        // Set mask byte control (bits 29:24)
        high |= ((mask as u32) & 0x3F) << GMACADDRH_MBC_SHIFT;

        // Set source address filter bit
        if source_addr {
            high |= GMACADDRH_SA;
        }

        // Enable the filter
        high |= GMACADDRH_AE;

        unsafe {
            write_reg(MAC_BASE + low_off, low);
            write_reg(MAC_BASE + high_off, high);
        }

        true
    }

    /// Clear a MAC address filter (disable slot)
    ///
    /// # Arguments
    /// * `slot` - Filter slot (1-4)
    ///
    /// # Returns
    /// `true` if successful, `false` if slot is invalid
    pub fn clear_mac_filter(slot: usize) -> bool {
        let Some((high_off, low_off)) = Self::addr_filter_offsets(slot) else {
            return false;
        };

        unsafe {
            write_reg(MAC_BASE + low_off, 0);
            write_reg(MAC_BASE + high_off, 0); // AE = 0 disables the filter
        }

        true
    }

    /// Check if a MAC address filter slot is enabled
    ///
    /// # Arguments
    /// * `slot` - Filter slot (1-4)
    ///
    /// # Returns
    /// `Some(true)` if enabled, `Some(false)` if disabled, `None` if invalid slot
    pub fn is_mac_filter_enabled(slot: usize) -> Option<bool> {
        let (high_off, _) = Self::addr_filter_offsets(slot)?;
        let high = unsafe { read_reg(MAC_BASE + high_off) };
        Some((high & GMACADDRH_AE) != 0)
    }

    /// Get a MAC address from a filter slot
    ///
    /// # Arguments
    /// * `slot` - Filter slot (1-4)
    ///
    /// # Returns
    /// `Some((addr, enabled))` or `None` if invalid slot
    pub fn get_mac_filter(slot: usize) -> Option<([u8; 6], bool)> {
        let (high_off, low_off) = Self::addr_filter_offsets(slot)?;

        let low = unsafe { read_reg(MAC_BASE + low_off) };
        let high = unsafe { read_reg(MAC_BASE + high_off) };

        let addr = [
            (low & 0xFF) as u8,
            ((low >> 8) & 0xFF) as u8,
            ((low >> 16) & 0xFF) as u8,
            ((low >> 24) & 0xFF) as u8,
            (high & 0xFF) as u8,
            ((high >> 8) & 0xFF) as u8,
        ];

        let enabled = (high & GMACADDRH_AE) != 0;
        Some((addr, enabled))
    }

    /// Clear all MAC address filters (slots 1-4)
    pub fn clear_all_mac_filters() {
        for slot in 1..=MAC_ADDR_FILTER_COUNT {
            Self::clear_mac_filter(slot);
        }
    }

    /// Find a free MAC address filter slot
    ///
    /// # Returns
    /// `Some(slot)` with the first available slot (1-4), or `None` if all are in use
    pub fn find_free_mac_filter_slot() -> Option<usize> {
        (1..=MAC_ADDR_FILTER_COUNT).find(|&slot| Self::is_mac_filter_enabled(slot) == Some(false))
    }

    /// Find a MAC address in the filter slots
    ///
    /// # Returns
    /// `Some(slot)` if the address is found, `None` otherwise
    pub fn find_mac_filter(addr: &[u8; 6]) -> Option<usize> {
        for slot in 1..=MAC_ADDR_FILTER_COUNT {
            if let Some((filter_addr, enabled)) = Self::get_mac_filter(slot)
                && enabled
                && filter_addr == *addr
            {
                return Some(slot);
            }
        }
        None
    }
}
