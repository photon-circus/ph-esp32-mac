//! MAC address, hash table, and VLAN filtering for the ESP32 EMAC.
//!
//! This module extends [`Emac`] with filtering capabilities:
//!
//! - **Perfect MAC filtering** - Up to 4 additional MAC address filters
//! - **Hash-based filtering** - 64-bit hash table for multicast groups
//! - **VLAN filtering** - 802.1Q VLAN tag filtering
//!
//! # Perfect MAC Filtering
//!
//! The ESP32 EMAC supports up to 4 additional MAC address filters beyond the
//! primary address. This allows receiving frames addressed to multiple unicast
//! or multicast addresses without enabling promiscuous mode.
//!
//! # Hash Filtering
//!
//! For subscribing to many multicast groups, hash filtering is more efficient.
//! The 64-bit hash table uses a CRC-based index. Note that collisions are
//! possible - multiple addresses may map to the same bit.
//!
//! # VLAN Filtering
//!
//! The MAC can filter frames based on 802.1Q VLAN tags, accepting only frames
//! with a specific VLAN ID.
//!
//! # Testing Notes
//!
//! These filtering features are advanced and have limited hardware validation
//! so far. Treat them as best-effort until broader testing confirms behavior.

use super::config::{MacAddressFilter, MacFilterType};
use super::emac::Emac;
use super::error::{ConfigError, DmaError, Result};
use crate::internal::register::mac::MacRegs;

// =============================================================================
// MAC Address Filtering
// =============================================================================

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    /// Add a MAC address filter
    ///
    /// The ESP32 supports up to 4 additional MAC address filters (beyond the
    /// primary address). This allows receiving frames addressed to multiple
    /// unicast or multicast addresses without enabling promiscuous mode.
    ///
    /// # Arguments
    /// * `addr` - MAC address to accept
    ///
    /// # Returns
    /// * `Ok(slot)` - The filter slot (1-4) where the address was added
    /// * `Err(NoDescriptorsAvailable)` - All 4 filter slots are in use
    ///
    /// # Example
    /// ```ignore
    /// // Accept frames addressed to a multicast group
    /// emac.add_mac_filter(&[0x01, 0x00, 0x5E, 0x00, 0x00, 0x01])?;
    /// ```
    pub fn add_mac_filter(&mut self, addr: &[u8; 6]) -> Result<usize> {
        // Check if already in filter
        if MacRegs::find_mac_filter(addr).is_some() {
            return Err(ConfigError::AlreadyInitialized.into());
        }

        // Find a free slot
        let slot = MacRegs::find_free_mac_filter_slot().ok_or(DmaError::NoDescriptorsAvailable)?;

        // Add the filter (destination address, no mask)
        MacRegs::set_mac_filter(slot, addr, false, 0);

        Ok(slot)
    }

    /// Add a MAC address filter with full configuration
    ///
    /// # Arguments
    /// * `filter` - Complete filter configuration including type and mask
    ///
    /// # Returns
    /// * `Ok(slot)` - The filter slot (1-4) where the filter was added
    /// * `Err(NoDescriptorsAvailable)` - All 4 filter slots are in use
    pub fn add_mac_filter_config(&mut self, filter: &MacAddressFilter) -> Result<usize> {
        // Check if already in filter
        if MacRegs::find_mac_filter(&filter.address).is_some() {
            return Err(ConfigError::AlreadyInitialized.into());
        }

        // Find a free slot
        let slot = MacRegs::find_free_mac_filter_slot().ok_or(DmaError::NoDescriptorsAvailable)?;

        let is_source = matches!(filter.filter_type, MacFilterType::Source);
        MacRegs::set_mac_filter(slot, &filter.address, is_source, filter.byte_mask);

        Ok(slot)
    }

    /// Remove a MAC address from the filter
    ///
    /// # Arguments
    /// * `addr` - MAC address to remove
    ///
    /// # Returns
    /// * `Ok(())` - Address was found and removed
    /// * `Err(InvalidLength)` - Address was not in the filter
    pub fn remove_mac_filter(&mut self, addr: &[u8; 6]) -> Result<()> {
        let slot = MacRegs::find_mac_filter(addr).ok_or(DmaError::InvalidLength)?;

        MacRegs::clear_mac_filter(slot);
        Ok(())
    }

    /// Clear all MAC address filters
    ///
    /// This removes all additional address filters, leaving only the primary
    /// MAC address active.
    pub fn clear_mac_filters(&mut self) {
        MacRegs::clear_all_mac_filters();
    }

    /// Get the number of active MAC address filters
    pub fn mac_filter_count(&self) -> usize {
        let mut count = 0;
        for slot in 1..=4 {
            if MacRegs::is_mac_filter_enabled(slot) == Some(true) {
                count += 1;
            }
        }
        count
    }

    /// Check if there are free MAC filter slots available
    pub fn has_free_mac_filter_slot(&self) -> bool {
        MacRegs::find_free_mac_filter_slot().is_some()
    }
}

// =============================================================================
// Hash Table Filtering
// =============================================================================

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    /// Add a MAC address to the hash filter
    ///
    /// The hash filter provides an efficient way to filter multiple multicast
    /// addresses without using the limited perfect filter slots. It uses a
    /// 64-bit hash table where each address maps to one bit.
    ///
    /// **Note:** Hash collisions are possible - multiple addresses may map to
    /// the same bit, causing some unwanted frames to pass through. This is
    /// acceptable for multicast filtering where software can do final filtering.
    ///
    /// # Arguments
    /// * `addr` - MAC address to add to the hash filter
    ///
    /// # Returns
    /// The hash index (0-63) where the address was added
    ///
    /// # Example
    /// ```ignore
    /// // Subscribe to IPv4 multicast group 224.0.0.1
    /// let multicast_addr = [0x01, 0x00, 0x5E, 0x00, 0x00, 0x01];
    /// emac.add_hash_filter(&multicast_addr);
    /// ```
    pub fn add_hash_filter(&mut self, addr: &[u8; 6]) -> u8 {
        let index = MacRegs::compute_hash_index(addr);
        MacRegs::set_hash_bit(index);
        index
    }

    /// Remove a MAC address from the hash filter
    ///
    /// **Warning:** If multiple addresses hash to the same bit, removing one
    /// will affect all of them. Consider tracking reference counts externally
    /// if you need precise removal behavior.
    ///
    /// # Arguments
    /// * `addr` - MAC address to remove from the hash filter
    ///
    /// # Returns
    /// The hash index (0-63) that was cleared
    pub fn remove_hash_filter(&mut self, addr: &[u8; 6]) -> u8 {
        let index = MacRegs::compute_hash_index(addr);
        MacRegs::clear_hash_bit(index);
        index
    }

    /// Check if a MAC address would pass the hash filter
    ///
    /// # Arguments
    /// * `addr` - MAC address to check
    ///
    /// # Returns
    /// `true` if the address's hash bit is set
    pub fn check_hash_filter(&self, addr: &[u8; 6]) -> bool {
        let index = MacRegs::compute_hash_index(addr);
        MacRegs::is_hash_bit_set(index)
    }

    /// Clear the entire hash table
    ///
    /// This disables hash-based filtering for all addresses.
    pub fn clear_hash_table(&mut self) {
        MacRegs::clear_hash_table();
    }

    /// Get the current hash table value
    pub fn hash_table(&self) -> u64 {
        MacRegs::hash_table()
    }

    /// Set the entire hash table at once
    ///
    /// Useful for restoring a saved state or bulk configuration.
    pub fn set_hash_table(&mut self, value: u64) {
        MacRegs::set_hash_table(value);
    }

    /// Enable hash-based multicast filtering
    ///
    /// When enabled, multicast frames are filtered using the hash table
    /// instead of passing all multicast (PM bit). This is more efficient
    /// for subscribing to specific multicast groups.
    ///
    /// # Arguments
    /// * `enable` - `true` to enable hash multicast filtering
    pub fn enable_hash_multicast(&mut self, enable: bool) {
        MacRegs::enable_hash_multicast(enable);

        // If enabling hash multicast, disable pass-all-multicast
        if enable {
            MacRegs::set_pass_all_multicast(false);
        }
    }

    /// Enable hash-based unicast filtering
    ///
    /// When enabled, unicast frames can be filtered using the hash table
    /// in addition to the perfect filter. This allows accepting frames
    /// for more unicast addresses than the perfect filter slots allow.
    ///
    /// # Arguments
    /// * `enable` - `true` to enable hash unicast filtering
    pub fn enable_hash_unicast(&mut self, enable: bool) {
        MacRegs::enable_hash_unicast(enable);
    }

    /// Compute hash index for a MAC address without modifying the table
    ///
    /// Useful for debugging or checking for potential collisions.
    pub fn compute_hash_index(addr: &[u8; 6]) -> u8 {
        MacRegs::compute_hash_index(addr)
    }
}

// =============================================================================
// VLAN Filtering
// =============================================================================

impl<const RX_BUFS: usize, const TX_BUFS: usize, const BUF_SIZE: usize>
    Emac<RX_BUFS, TX_BUFS, BUF_SIZE>
{
    /// Enable VLAN tag filtering with a specific VLAN ID
    ///
    /// Configures the MAC to filter frames based on the 802.1Q VLAN tag.
    /// Only frames with the specified VLAN ID will be received.
    ///
    /// # Arguments
    /// * `vid` - VLAN Identifier (0-4095)
    ///
    /// # Example
    /// ```ignore
    /// // Only receive frames from VLAN 100
    /// emac.set_vlan_filter(100);
    /// ```
    pub fn set_vlan_filter(&mut self, vid: u16) {
        MacRegs::set_vlan_id_filter(vid);
        MacRegs::enable_vlan_filter(true);
    }

    /// Configure VLAN filter with full options
    ///
    /// # Arguments
    /// * `vid` - VLAN Identifier (0-4095) or full 16-bit tag
    /// * `vid_only` - If true, compare only 12-bit VID; if false, compare full tag
    /// * `inverse` - If true, pass frames that DON'T match (exclusion filter)
    /// * `svlan` - If true, match S-VLAN (0x88A8); if false, match C-VLAN (0x8100)
    pub fn configure_vlan_filter(&mut self, vid: u16, vid_only: bool, inverse: bool, svlan: bool) {
        MacRegs::configure_vlan_filter(vid, vid_only, inverse, svlan);
        MacRegs::enable_vlan_filter(true);
    }

    /// Disable VLAN filtering
    ///
    /// After calling this, frames will not be filtered by VLAN tag.
    pub fn disable_vlan_filter(&mut self) {
        MacRegs::clear_vlan_filter();
    }

    /// Check if VLAN filtering is currently enabled
    pub fn is_vlan_filter_enabled(&self) -> bool {
        MacRegs::is_vlan_filter_enabled()
    }

    /// Get the currently configured VLAN ID filter
    ///
    /// Returns the 12-bit VLAN ID that is being filtered.
    pub fn vlan_filter_id(&self) -> u16 {
        MacRegs::get_vlan_id_filter()
    }
}
