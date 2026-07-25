//! Code-owned QA suite definitions and release gates.

use std::{fmt, str::FromStr};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A dedicated target-hardware validation suite.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Suite {
    /// Repeated SMI reads, reversible writes, and link transitions.
    Mdio,
    /// Driver lifecycle, reset recovery, and boot validation.
    Reset,
    /// Perfect-filter programming and runtime MAC changes.
    MacFilter,
    /// Synchronous RX-ring exhaustion and recovery.
    RxSync,
    /// Raw async RX-ring exhaustion and wake recovery.
    RxAsync,
    /// embassy-net exhaustion, UDP recovery, and DHCP behavior.
    RxEmbassy,
}

impl Suite {
    /// Returns the QA firmware binary name for this suite.
    #[must_use]
    pub const fn binary_name(self) -> &'static str {
        match self {
            Self::Mdio => "mdio",
            Self::Reset => "reset",
            Self::MacFilter => "mac-filter",
            Self::RxSync => "rx-sync",
            Self::RxAsync => "rx-async",
            Self::RxEmbassy => "rx-embassy",
        }
    }

    /// Returns the host-owned release definition for this suite.
    #[must_use]
    pub const fn definition(self) -> &'static SuiteDefinition {
        match self {
            Self::Mdio => &MDIO,
            Self::Reset => &RESET,
            Self::MacFilter => &MAC_FILTER,
            Self::RxSync => &RX_SYNC,
            Self::RxAsync => &RX_ASYNC,
            Self::RxEmbassy => &RX_EMBASSY,
        }
    }

    /// Returns the stable numeric suite code used in PHQA Ethernet frames.
    #[must_use]
    pub const fn wire_code(self) -> u8 {
        match self {
            Self::Mdio => 1,
            Self::Reset => 2,
            Self::MacFilter => 3,
            Self::RxSync => 4,
            Self::RxAsync => 5,
            Self::RxEmbassy => 6,
        }
    }

    /// Decodes a stable PHQA Ethernet suite code.
    #[must_use]
    pub const fn from_wire_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Mdio),
            2 => Some(Self::Reset),
            3 => Some(Self::MacFilter),
            4 => Some(Self::RxSync),
            5 => Some(Self::RxAsync),
            6 => Some(Self::RxEmbassy),
            _ => None,
        }
    }
}

impl fmt::Display for Suite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Mdio => "mdio",
            Self::Reset => "reset",
            Self::MacFilter => "mac-filter",
            Self::RxSync => "rx-sync",
            Self::RxAsync => "rx-async",
            Self::RxEmbassy => "rx-embassy",
        })
    }
}

impl FromStr for Suite {
    type Err = SuiteParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "mdio" => Ok(Self::Mdio),
            "reset" => Ok(Self::Reset),
            "mac-filter" => Ok(Self::MacFilter),
            "rx-sync" => Ok(Self::RxSync),
            "rx-async" => Ok(Self::RxAsync),
            "rx-embassy" => Ok(Self::RxEmbassy),
            _ => Err(SuiteParseError(value.to_owned())),
        }
    }
}

/// Failure returned when a suite name is not recognized.
#[derive(Debug, Error)]
#[error("unknown QA suite `{0}`")]
pub struct SuiteParseError(String);

/// Immutable release expectations for a firmware suite.
#[derive(Debug)]
pub struct SuiteDefinition {
    /// Test IDs that must occur exactly once before `RUN_END`.
    pub required_tests: &'static [&'static str],
    /// Host actions that firmware may request through `READY`.
    pub allowed_actions: &'static [&'static str],
}

const MDIO: SuiteDefinition = SuiteDefinition {
    required_tests: &[
        "mdio.read-repeat",
        "mdio.phy-id",
        "mdio.bmsr-latch",
        "mdio.anar-roundtrip",
        "mdio.link-transition",
    ],
    allowed_actions: &["link_down", "link_up"],
};

const RESET: SuiteDefinition = SuiteDefinition {
    required_tests: &[
        "reset.lifecycle",
        "reset.second-init",
        "reset.missing-clock-retry",
        "reset.unicast",
        "reset.atomicity",
    ],
    allowed_actions: &["inject_unicast"],
};

const MAC_FILTER: SuiteDefinition = SuiteDefinition {
    required_tests: &[
        "mac.readback",
        "mac.promiscuous-off",
        "mac.device-unicast",
        "mac.broadcast",
        "mac.multicast",
        "mac.alien-rejected",
        "mac.runtime-change",
    ],
    allowed_actions: &[
        "inject_unicast",
        "inject_broadcast",
        "inject_multicast",
        "inject_alien",
        "inject_old_mac",
        "inject_new_mac",
    ],
};

const RX_SYNC: SuiteDefinition = SuiteDefinition {
    required_tests: &[
        "rx-sync.exhausted",
        "rx-sync.quiet",
        "rx-sync.flood",
        "rx-sync.heartbeat",
        "rx-sync.recovered",
    ],
    allowed_actions: &["fill_rx", "flood_rx", "send_sentinel"],
};

const RX_ASYNC: SuiteDefinition = SuiteDefinition {
    required_tests: &[
        "rx-async.exhausted",
        "rx-async.quiet",
        "rx-async.flood",
        "rx-async.heartbeat",
        "rx-async.recovered",
        "rx-async.second-wake",
    ],
    allowed_actions: &[
        "fill_rx",
        "flood_rx",
        "send_sentinel",
        "send_second_sentinel",
    ],
};

const RX_EMBASSY: SuiteDefinition = SuiteDefinition {
    required_tests: &[
        "rx-embassy.exhausted",
        "rx-embassy.quiet",
        "rx-embassy.flood",
        "rx-embassy.heartbeat",
        "rx-embassy.udp-recovered",
        "rx-embassy.dhcp",
    ],
    allowed_actions: &["fill_rx", "flood_rx", "udp_echo", "dhcp"],
};

#[cfg(test)]
mod tests {
    use super::Suite;

    #[test]
    fn names_round_trip() {
        for suite in [
            Suite::Mdio,
            Suite::Reset,
            Suite::MacFilter,
            Suite::RxSync,
            Suite::RxAsync,
            Suite::RxEmbassy,
        ] {
            assert_eq!(suite.to_string().parse::<Suite>().unwrap(), suite);
        }
    }
}
