//! QA Runner for WT32-ETH01 Board
//!
//! This binary runs a series of hardware QA tests on the WT32-ETH01
//! development board to verify the ph-esp32-mac driver functionality.
//!
//! # Test Groups
//!
//! | Group | ID Range | Category |
//! |-------|----------|----------|
//! | 1 | IT-1-xxx | Register Access |
//! | 2 | IT-2-xxx | EMAC Initialization |
//! | 3 | IT-3-xxx | PHY Communication |
//! | 4 | IT-4-xxx | EMAC Operations |
//! | 5 | IT-5-xxx | Link Status |
//! | 6 | IT-6-xxx | smoltcp Integration |
//! | 7 | IT-7-xxx | State & Interrupts |
//! | 8 | IT-8-xxx | Advanced Features |
//! | 9 | IT-9-xxx | Edge Cases |
//!
//! # Building and Flashing
//!
//! ```bash
//! cargo xtask run qa-runner
//! ```

#![no_std]
#![no_main]

mod tests;

use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    main,
};
use log::{error, info, warn};

esp_app_desc!();

// Import PhyDriver trait for method access
use ph_esp32_mac::PhyDriver;

// Re-export framework types for the run_test! macro
use tests::{TestContext, TestResult, TestStats, EMAC};

use ph_esp32_mac::boards::wt32_eth01::Wt32Eth01 as Board;

// =============================================================================
// Run Test Macro (with test ID)
// =============================================================================

/// Run a single test with ID, log the result, and record statistics
macro_rules! run_test {
    ($stats:expr, $id:expr, $name:expr, $test_fn:expr) => {{
        info!("");
        info!("▶ [{}] {}", $id, $name);
        let result = $test_fn;
        match result {
            TestResult::Pass => info!("  {} PASS", result.symbol()),
            TestResult::Fail => error!("  {} FAIL", result.symbol()),
            TestResult::Skip => warn!("  {} SKIP", result.symbol()),
        }
        $stats.record(result);
        result
    }};
}

// =============================================================================
// Main Entry Point
// =============================================================================

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    
    info!("");
    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║       WT32-ETH01 QA Runner                                   ║");
    info!("║       ph-esp32-mac Driver Verification                       ║");
    info!("╚══════════════════════════════════════════════════════════════╝");
    info!("");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Enable external oscillator
    info!("Enabling external 50MHz oscillator...");
    let clk_pin = Output::new(peripherals.GPIO16, Level::High, OutputConfig::default());
    esp_hal::delay::Delay::new().delay_millis(Board::OSC_STARTUP_MS);
    info!(
        "Oscillator enabled (GPIO{} = HIGH)",
        Board::CLK_EN_GPIO
    );
    info!("");

    let mut stats = TestStats::new();
    let mut ctx = TestContext::new(clk_pin);

    // =========================================================================
    // Test Group 1: Register Access
    // =========================================================================
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 1: Register Access");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    run_test!(stats, "IT-1-001", "EMAC clock enable", tests::group1_register::test_emac_clock_enable());
    run_test!(stats, "IT-1-002", "DMA registers", tests::group1_register::test_dma_registers_accessible());
    run_test!(stats, "IT-1-003", "MAC registers", tests::group1_register::test_mac_registers_accessible());
    run_test!(stats, "IT-1-004", "Extension registers", tests::group1_register::test_extension_registers());

    // =========================================================================
    // Test Group 2: EMAC Initialization  
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 2: EMAC Initialization");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    let emac_ok = run_test!(stats, "IT-2-001", "EMAC init", tests::group2_init::test_emac_init(&mut ctx)) == TestResult::Pass;
    
    if emac_ok {
        run_test!(stats, "IT-2-002", "RMII pin config", tests::group2_init::test_rmii_pins());
        run_test!(stats, "IT-2-003", "DMA descriptor chain", tests::group2_init::test_dma_descriptor_chain());
    } else {
        warn!("  Skipping dependent tests");
        stats.record(TestResult::Skip);
        stats.record(TestResult::Skip);
    }

    // =========================================================================
    // Test Group 3: PHY Communication
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 3: PHY Communication");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    run_test!(stats, "IT-3-001", "PHY MDIO read", tests::group3_phy::test_phy_mdio_read(&mut ctx));
    run_test!(stats, "IT-3-002", "PHY init", tests::group3_phy::test_phy_init(&mut ctx));
    let link_ok = run_test!(stats, "IT-3-003", "PHY link up", tests::group3_phy::test_phy_link_up(&mut ctx, 5000)) == TestResult::Pass;

    // =========================================================================
    // Test Group 4: EMAC Operations
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 4: EMAC Operations");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    if emac_ok && link_ok {
        run_test!(stats, "IT-4-001", "EMAC start", tests::group4_emac::test_emac_start());
        run_test!(stats, "IT-4-002", "Packet TX", tests::group4_emac::test_packet_tx());
        run_test!(stats, "IT-4-003", "Packet RX (3s)", tests::group4_emac::test_packet_rx(3));
        run_test!(stats, "IT-4-004", "EMAC stop/start", tests::group4_emac::test_emac_stop_start());
    } else {
        warn!("  Skipping - requires EMAC init and link");
        for _ in 0..4 { stats.record(TestResult::Skip); }
    }

    // =========================================================================
    // Test Group 5: Link Status
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 5: Link Status");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    run_test!(stats, "IT-5-001", "Link status query", tests::group5_link::test_link_status_query(&mut ctx));

    // =========================================================================
    // Test Group 6: smoltcp Integration
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 6: smoltcp Integration");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    if emac_ok && link_ok {
        run_test!(stats, "IT-6-001", "Interface creation", tests::group6_smoltcp::test_interface_creation());
        run_test!(stats, "IT-6-002", "Device capabilities", tests::group6_smoltcp::test_device_capabilities());
        run_test!(stats, "IT-6-003", "Interface poll", tests::group6_smoltcp::test_interface_poll());
    } else {
        warn!("  Skipping - requires EMAC init and link");
        for _ in 0..3 { stats.record(TestResult::Skip); }
    }

    // =========================================================================
    // Test Group 7: State & Interrupts
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 7: State & Interrupts");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    if emac_ok && link_ok {
        run_test!(stats, "IT-7-001", "State transitions", tests::group7_state::test_state_transitions());
        run_test!(stats, "IT-7-002", "State stop changes", tests::group7_state::test_state_stop_changes());
        run_test!(stats, "IT-7-003", "TX ready", tests::group7_state::test_tx_ready());
        run_test!(stats, "IT-7-004", "Can transmit sizes", tests::group7_state::test_can_transmit());
        run_test!(stats, "IT-7-005", "TX backpressure", tests::group7_state::test_tx_backpressure());
        run_test!(stats, "IT-7-006", "Peek RX length", tests::group7_state::test_peek_rx_length());
        run_test!(stats, "IT-7-007", "RX frames waiting", tests::group7_state::test_rx_frames_waiting());
        run_test!(stats, "IT-7-008", "Interrupt status", tests::group7_state::test_interrupt_status());
        run_test!(stats, "IT-7-009", "Interrupt clear", tests::group7_state::test_interrupt_clear());
        run_test!(stats, "IT-7-010", "Handle interrupt", tests::group7_state::test_handle_interrupt());
        run_test!(stats, "IT-7-011", "Frame sizes TX", tests::group7_state::test_frame_sizes());
    } else {
        warn!("  Skipping - requires EMAC init and link");
        for _ in 0..11 { stats.record(TestResult::Skip); }
    }

    // =========================================================================
    // Test Group 8: Advanced Features
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 8: Advanced Features");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    if emac_ok && link_ok {
        run_test!(stats, "IT-8-001", "Promiscuous mode", tests::group8_advanced::test_promiscuous_mode());
        run_test!(stats, "IT-8-002", "Promiscuous RX", tests::group8_advanced::test_promiscuous_rx(2000));
        run_test!(stats, "IT-8-003", "PHY capabilities", tests::group8_advanced::test_phy_capabilities(&mut ctx));
        run_test!(stats, "IT-8-004", "Force link", tests::group8_advanced::test_force_link(&mut ctx));
        run_test!(stats, "IT-8-005", "Enable TX interrupt", tests::group8_advanced::test_enable_tx_interrupt());
        run_test!(stats, "IT-8-006", "Enable RX interrupt", tests::group8_advanced::test_enable_rx_interrupt());
        run_test!(stats, "IT-8-007", "TX interrupt fires", tests::group8_advanced::test_tx_interrupt_fires());
    } else {
        warn!("  Skipping - requires EMAC init and link");
        for _ in 0..7 { stats.record(TestResult::Skip); }
    }

    // =========================================================================
    // Test Group 9: Edge Cases
    // =========================================================================
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("  GROUP 9: Edge Cases");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    if emac_ok && link_ok {
        run_test!(stats, "IT-9-001", "MAC filtering", tests::group9_edge::test_mac_filtering());
        run_test!(stats, "IT-9-002", "MAC filter multiple", tests::group9_edge::test_mac_filter_multiple());
        run_test!(stats, "IT-9-003", "Hash filtering", tests::group9_edge::test_hash_filtering());
        run_test!(stats, "IT-9-004", "Pass all multicast", tests::group9_edge::test_pass_all_multicast());
        run_test!(stats, "IT-9-005", "VLAN filtering", tests::group9_edge::test_vlan_filtering());
        run_test!(stats, "IT-9-006", "Flow control config", tests::group9_edge::test_flow_control_config());
        run_test!(stats, "IT-9-007", "Flow control check", tests::group9_edge::test_flow_control_check());
        run_test!(stats, "IT-9-008", "PHY energy detect", tests::group9_edge::test_energy_detect(&mut ctx));
        run_test!(stats, "IT-9-009", "RX interrupt fires", tests::group9_edge::test_rx_interrupt_fires(2000));
        run_test!(stats, "IT-9-010", "Async wakers", tests::group9_edge::test_async_wakers());
        run_test!(stats, "IT-9-011", "Restore RX state", tests::group9_edge::test_restore_rx_state());
    } else {
        warn!("  Skipping - requires EMAC init and link");
        for _ in 0..11 { stats.record(TestResult::Skip); }
    }

    // =========================================================================
    // Test Summary
    // =========================================================================
    info!("");
    info!("══════════════════════════════════════════════════════════════════");
    info!("  TEST SUMMARY");
    info!("══════════════════════════════════════════════════════════════════");
    info!("");
    info!("  Total:   {}", stats.total());
    info!("  Passed:  {} ✓", stats.passed);
    info!("  Failed:  {} ✗", stats.failed);
    info!("  Skipped: {} ○", stats.skipped);
    info!("");
    
    if stats.all_passed() {
        info!("╔══════════════════════════════════════════════════════════════╗");
        info!("║                    ALL TESTS PASSED! ✓                       ║");
        info!("╚══════════════════════════════════════════════════════════════╝");
    } else {
        error!("╔══════════════════════════════════════════════════════════════╗");
        error!("║                  SOME TESTS FAILED! ✗                        ║");
        error!("╚══════════════════════════════════════════════════════════════╝");
    }
    info!("");

    // =========================================================================
    // Continuous Monitoring Mode
    // =========================================================================
    info!("Entering continuous RX monitoring mode...");
    info!("(Press reset to restart tests)");
    info!("");

    let mut rx_buffer = [0u8; 1600];
    let mut packet_count = 0u32;
    let mut last_report = 0u32;

    loop {
        critical_section::with(|cs| {
            if let Some(ref mut emac) = *EMAC.borrow_ref_mut(cs) {
                while emac.rx_available() {
                    if let Ok(len) = emac.receive(&mut rx_buffer) {
                        packet_count += 1;
                        if len >= 14 {
                            let src = &rx_buffer[6..12];
                            let dst = &rx_buffer[0..6];
                            let etype = u16::from_be_bytes([rx_buffer[12], rx_buffer[13]]);
                            
                            let type_str = match etype {
                                0x0800 => "IPv4",
                                0x0806 => "ARP",
                                0x86DD => "IPv6",
                                0x88CC => "LLDP",
                                _ => "",
                            };
                            
                            info!("RX #{}: {}B {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}->{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} [0x{:04X}] {}",
                                packet_count, len,
                                src[0], src[1], src[2], src[3], src[4], src[5],
                                dst[0], dst[1], dst[2], dst[3], dst[4], dst[5],
                                etype, type_str);
                        }
                    }
                }
            }
        });

        last_report += 1;
        if last_report >= 10000 {
            last_report = 0;
            info!("--- {} packets total ---", packet_count);
            
            if let Ok(up) = ctx.phy.is_link_up(&mut ctx.mdio) {
                info!("Link: {}", if up { "UP" } else { "DOWN" });
            }
        }

        esp_hal::delay::Delay::new().delay_micros(100);
    }
}
