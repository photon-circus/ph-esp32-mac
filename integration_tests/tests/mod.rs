//! Integration Test Modules
//!
//! This module organizes all integration tests with unique IDs for reference.
//!
//! # Test ID Format
//!
//! Test IDs follow the pattern: `IT-{GROUP}-{NUMBER}`
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

pub mod framework;
pub mod group1_register;
pub mod group2_init;
pub mod group3_phy;
pub mod group4_emac;
pub mod group5_link;
pub mod group6_smoltcp;
pub mod group7_state;
pub mod group8_advanced;
pub mod group9_edge;

// Re-export everything needed
pub use framework::*;
