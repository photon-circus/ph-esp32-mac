//! Strict, shell-free lab configuration.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported lab configuration schema.
pub const LAB_SCHEMA: u32 = 1;

/// A process command represented as an executable followed by literal argv.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CommandArgv(Vec<String>);

impl CommandArgv {
    /// Constructs and validates an argv vector.
    pub fn new(values: Vec<String>) -> Result<Self, ConfigError> {
        let argv = Self(values);
        argv.validate("command")?;
        Ok(argv)
    }

    /// Returns the executable name or path.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.0[0]
    }

    /// Returns literal arguments without invoking a command shell.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.0[1..]
    }

    /// Converts the command to OS-native arguments.
    #[must_use]
    pub fn os_values(&self) -> Vec<OsString> {
        self.0.iter().map(OsString::from).collect()
    }

    fn validate(&self, field: &'static str) -> Result<(), ConfigError> {
        if self.0.is_empty() || self.0.iter().any(|part| part.is_empty()) {
            return Err(ConfigError::EmptyCommand(field));
        }
        Ok(())
    }
}

/// Serial connection settings for the ESP32 console.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SerialConfig {
    /// Windows COM port, for example `COM7`.
    pub port: String,
    /// UART line rate.
    pub baud: u32,
    /// Upper bound for one serial read.
    pub read_timeout_ms: u64,
    /// Time allowed for a new firmware boot to emit `RUN_START`.
    pub boot_timeout_ms: u64,
}

/// Packet capture and injection settings.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PacketConfig {
    /// Npcap device name, normally `\Device\NPF_{GUID}`.
    pub adapter: String,
    /// Host interface MAC used in injected frames.
    pub host_mac: String,
    /// Primary DUT MAC compiled into QA firmware.
    pub dut_mac: String,
    /// Npcap read timeout.
    pub capture_timeout_ms: i32,
}

impl PacketConfig {
    /// Parses the configured host MAC.
    pub fn parsed_host_mac(&self) -> Result<[u8; 6], ConfigError> {
        parse_mac("packet.host_mac", &self.host_mac)
    }

    /// Parses the configured DUT MAC.
    pub fn parsed_dut_mac(&self) -> Result<[u8; 6], ConfigError> {
        parse_mac("packet.dut_mac", &self.dut_mac)
    }
}

/// External tools used by the controller.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    /// `espflash` executable and any fixed arguments.
    pub espflash: CommandArgv,
}

/// Required relay and link-control commands.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConfig {
    /// Remove power from the board.
    pub power_off: CommandArgv,
    /// Apply power to the board.
    pub power_on: CommandArgv,
    /// Disable the Ethernet link.
    pub link_down: CommandArgv,
    /// Restore the Ethernet link.
    pub link_up: CommandArgv,
}

/// Time limits for external processes and complete suites.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimeoutConfig {
    /// Maximum duration of a relay or link-control command.
    pub action_ms: u64,
    /// Maximum duration of an `espflash` invocation.
    pub flash_ms: u64,
    /// Maximum duration of one firmware session.
    pub suite_ms: u64,
}

/// Hardware settling delays.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettleConfig {
    /// Delay after applying board power.
    pub power_ms: u64,
    /// Delay after changing link state.
    pub link_ms: u64,
}

/// Complete lab definition.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LabConfig {
    /// Configuration schema version.
    pub schema: u32,
    /// Stable physical board identifier for evidence.
    pub board_id: String,
    /// Serial connection.
    pub serial: SerialConfig,
    /// Packet capture and injection.
    pub packet: PacketConfig,
    /// External tool commands.
    pub tools: ToolConfig,
    /// Relay and link-control commands.
    pub actions: ActionConfig,
    /// Operation time limits.
    pub timeouts: TimeoutConfig,
    /// Hardware settling delays.
    pub settle: SettleConfig,
    #[serde(skip)]
    source_path: PathBuf,
}

impl LabConfig {
    /// Loads and validates a TOML lab definition.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut config: Self = toml::from_str(&text).map_err(ConfigError::Toml)?;
        config.source_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        config.validate()?;
        Ok(config)
    }

    /// Validates all fields that cannot be expressed in the TOML schema.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema != LAB_SCHEMA {
            return Err(ConfigError::Schema(self.schema));
        }
        if self.board_id.trim().is_empty() {
            return Err(ConfigError::EmptyValue("board_id"));
        }
        if self.serial.port.trim().is_empty() {
            return Err(ConfigError::EmptyValue("serial.port"));
        }
        if self.serial.baud == 0 {
            return Err(ConfigError::ZeroValue("serial.baud"));
        }
        if self.serial.read_timeout_ms == 0 {
            return Err(ConfigError::ZeroValue("serial.read_timeout_ms"));
        }
        if self.serial.boot_timeout_ms == 0 {
            return Err(ConfigError::ZeroValue("serial.boot_timeout_ms"));
        }
        if self.packet.adapter.trim().is_empty() {
            return Err(ConfigError::EmptyValue("packet.adapter"));
        }
        self.packet.parsed_host_mac()?;
        self.packet.parsed_dut_mac()?;
        if self.packet.capture_timeout_ms <= 0 {
            return Err(ConfigError::NonPositiveValue("packet.capture_timeout_ms"));
        }
        self.tools.espflash.validate("tools.espflash")?;
        self.actions.power_off.validate("actions.power_off")?;
        self.actions.power_on.validate("actions.power_on")?;
        self.actions.link_down.validate("actions.link_down")?;
        self.actions.link_up.validate("actions.link_up")?;
        if self.timeouts.action_ms == 0 {
            return Err(ConfigError::ZeroValue("timeouts.action_ms"));
        }
        if self.timeouts.flash_ms == 0 {
            return Err(ConfigError::ZeroValue("timeouts.flash_ms"));
        }
        if self.timeouts.suite_ms == 0 {
            return Err(ConfigError::ZeroValue("timeouts.suite_ms"));
        }
        Ok(())
    }

    /// Returns the directory containing the lab file.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        self.source_path.parent().unwrap_or_else(|| Path::new("."))
    }

    /// Resolves a configured executable relative to the lab file when needed.
    #[must_use]
    pub fn resolve_executable(&self, command: &CommandArgv) -> PathBuf {
        let executable = Path::new(command.executable());
        if executable.is_absolute()
            || (!command.executable().contains('/') && !command.executable().contains('\\'))
        {
            executable.to_path_buf()
        } else {
            self.base_dir().join(executable)
        }
    }
}

/// Lab configuration failure.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Lab file could not be read.
    #[error("failed to read lab config {}: {source}", path.display())]
    Read {
        /// Path being read.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// TOML syntax or schema was invalid.
    #[error("invalid lab TOML: {0}")]
    Toml(toml::de::Error),
    /// Schema version is unsupported.
    #[error("unsupported lab schema {0}; expected {LAB_SCHEMA}")]
    Schema(u32),
    /// Required command argv was empty.
    #[error("`{0}` must be a nonempty executable-plus-arguments array")]
    EmptyCommand(&'static str),
    /// Required text was empty.
    #[error("`{0}` must not be empty")]
    EmptyValue(&'static str),
    /// Numeric value must be nonzero.
    #[error("`{0}` must be greater than zero")]
    ZeroValue(&'static str),
    /// Signed numeric value must be positive.
    #[error("`{0}` must be positive")]
    NonPositiveValue(&'static str),
    /// MAC address syntax was invalid.
    #[error("`{field}` is not a six-byte colon-separated MAC address: `{value}`")]
    InvalidMac {
        /// Configuration field.
        field: &'static str,
        /// Rejected value.
        value: String,
    },
}

fn parse_mac(field: &'static str, value: &str) -> Result<[u8; 6], ConfigError> {
    let mut parsed = [0_u8; 6];
    let mut count = 0;
    for (index, component) in value.split(':').enumerate() {
        if index >= parsed.len() || component.len() != 2 {
            return Err(ConfigError::InvalidMac {
                field,
                value: value.to_owned(),
            });
        }
        parsed[index] = u8::from_str_radix(component, 16).map_err(|_| ConfigError::InvalidMac {
            field,
            value: value.to_owned(),
        })?;
        count += 1;
    }
    if count != parsed.len() {
        return Err(ConfigError::InvalidMac {
            field,
            value: value.to_owned(),
        });
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{ConfigError, LabConfig};

    const VALID: &str = r#"
schema = 1
board_id = "wt32-a"

[serial]
port = "COM7"
baud = 115200
read_timeout_ms = 50
boot_timeout_ms = 10000

[packet]
adapter = "\\Device\\NPF_{00000000-0000-0000-0000-000000000000}"
host_mac = "02:00:00:00:00:01"
dut_mac = "02:00:00:00:00:02"
capture_timeout_ms = 50

[tools]
espflash = ["espflash", "--skip-update-check"]

[actions]
power_off = ["relay.exe", "off"]
power_on = ["relay.exe", "on"]
link_down = ["switch.exe", "down"]
link_up = ["switch.exe", "up"]

[timeouts]
action_ms = 10000
flash_ms = 120000
suite_ms = 60000

[settle]
power_ms = 500
link_ms = 500
"#;

    fn load(text: &str) -> Result<LabConfig, ConfigError> {
        let temp = tempdir().unwrap();
        let path = temp.path().join("lab.toml");
        fs::write(&path, text).unwrap();
        LabConfig::load(path)
    }

    #[test]
    fn loads_argv_arrays_without_a_shell() {
        let config = load(VALID).unwrap();
        assert_eq!(config.actions.power_off.executable(), "relay.exe");
        assert_eq!(config.actions.power_off.arguments(), ["off"]);
        assert_eq!(config.packet.parsed_dut_mac().unwrap()[5], 2);
    }

    #[test]
    fn rejects_shell_command_strings() {
        let invalid = VALID.replace(
            "power_off = [\"relay.exe\", \"off\"]",
            "power_off = \"relay.exe off\"",
        );
        assert!(matches!(load(&invalid), Err(ConfigError::Toml(_))));
    }

    #[test]
    fn rejects_missing_required_action() {
        let invalid = VALID.replace("power_on = [\"relay.exe\", \"on\"]\n", "");
        assert!(matches!(load(&invalid), Err(ConfigError::Toml(_))));
    }

    #[test]
    fn rejects_unknown_fields() {
        let invalid = VALID.replace("board_id = \"wt32-a\"", "board_id = \"wt32-a\"\ntyop = 1");
        assert!(matches!(load(&invalid), Err(ConfigError::Toml(_))));
    }

    #[test]
    fn rejects_invalid_mac() {
        let invalid = VALID.replace("dut_mac = \"02:00:00:00:00:02\"", "dut_mac = \"not-a-mac\"");
        assert!(matches!(
            load(&invalid),
            Err(ConfigError::InvalidMac { .. })
        ));
    }
}
