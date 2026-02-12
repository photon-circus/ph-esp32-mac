//! Group 5: Link Status Tests
//!
//! PHY link monitoring capability.
//!
//! | Test ID | Name | Description |
//! |---------|------|-------------|
//! | IT-5-001 | Link status query | Check PHY link up/down |

use log::{error, info, warn};
use ph_esp32_mac::PhyDriver;

use super::framework::{TestContext, TestResult};

/// IT-5-001: Test link status query
pub fn test_link_status_query(ctx: &mut TestContext) -> TestResult {
    match ctx.phy.is_link_up(&mut ctx.mdio) {
        Ok(true) => {
            info!("  Link: UP");
            TestResult::Pass
        }
        Ok(false) => {
            warn!("  Link: DOWN");
            TestResult::Skip
        }
        Err(e) => {
            error!("  Link query failed: {:?}", e);
            TestResult::Fail
        }
    }
}
