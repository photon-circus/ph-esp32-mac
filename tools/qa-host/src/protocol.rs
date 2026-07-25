//! Allocation-tolerant host parser for the allocation-free firmware protocol.
//!
//! Firmware records are ASCII lines beginning with `PHQA|1|`. Diagnostic
//! output that does not use that prefix is retained by the evidence layer but
//! ignored by this parser. A malformed prefixed line is always an error.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum accepted machine-protocol line length, excluding its newline.
pub const MAX_LINE_LEN: usize = 1_024;

/// Firmware execution mode declared at the start of a run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    /// Exploratory smoke testing whose skips are informational.
    Exploratory,
    /// Strict release validation.
    Release,
}

/// Result attached to a single firmware test.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TestStatus {
    /// The requirement was demonstrated.
    Pass,
    /// The requirement was attempted and failed.
    Fail,
    /// The requirement was not attempted.
    Skip,
    /// The requirement cannot yet be demonstrated.
    Blocked,
}

/// Result claimed by firmware at the end of a run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RunResult {
    /// Every required test passed.
    Pass,
    /// At least one required condition failed.
    Fail,
}

/// One parsed PHQA protocol record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Record {
    /// The first record emitted by a firmware boot.
    RunStart {
        /// Random identifier that scopes all records from this boot.
        run_id: u64,
        /// Compile-time suite name.
        suite: String,
        /// Full source commit embedded in the firmware.
        commit: String,
        /// Whether firmware is running exploratory or release rules.
        mode: RunMode,
        /// Reset reason captured before board initialization.
        reset_reason: String,
    },
    /// A synchronization point requesting one host action.
    Ready {
        /// Run identifier copied from `RUN_START`.
        run_id: u64,
        /// Strictly increasing action step.
        step: u32,
        /// Code-owned action name.
        action: String,
    },
    /// A named hardware observation.
    Observation {
        /// Run identifier copied from `RUN_START`.
        run_id: u64,
        /// Stable observation name.
        name: String,
        /// Token-form observation value.
        value: String,
    },
    /// One test outcome.
    Test {
        /// Run identifier copied from `RUN_START`.
        run_id: u64,
        /// Stable test identifier.
        id: String,
        /// Firmware-observed status.
        status: TestStatus,
        /// Whether a skip or block invalidates release evidence.
        required: bool,
        /// Machine-readable reason for a non-pass status.
        reason: Option<String>,
    },
    /// The final record emitted by a completed suite.
    RunEnd {
        /// Run identifier copied from `RUN_START`.
        run_id: u64,
        /// Aggregate result claimed by firmware.
        result: RunResult,
    },
}

impl Record {
    /// Returns the run identifier carried by this record.
    #[must_use]
    pub const fn run_id(&self) -> u64 {
        match self {
            Self::RunStart { run_id, .. }
            | Self::Ready { run_id, .. }
            | Self::Observation { run_id, .. }
            | Self::Test { run_id, .. }
            | Self::RunEnd { run_id, .. } => *run_id,
        }
    }
}

/// Protocol framing or field validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProtocolError {
    /// A protocol line exceeded the fixed safety limit.
    #[error("PHQA line exceeds {MAX_LINE_LEN} bytes")]
    LineTooLong,
    /// Input ended with an unterminated line.
    #[error("serial input ended with a truncated line")]
    TruncatedLine,
    /// The protocol version is unsupported.
    #[error("unsupported PHQA protocol version `{0}`")]
    UnsupportedVersion(String),
    /// The record kind is unknown.
    #[error("unknown PHQA record kind `{0}`")]
    UnknownKind(String),
    /// A field did not contain a key and value.
    #[error("invalid PHQA field `{0}`")]
    InvalidField(String),
    /// A key occurred more than once.
    #[error("duplicate PHQA field `{0}`")]
    DuplicateField(String),
    /// A mandatory field was absent.
    #[error("missing PHQA field `{0}`")]
    MissingField(&'static str),
    /// A record supplied a field that its schema does not define.
    #[error("unexpected PHQA field `{0}`")]
    UnexpectedField(String),
    /// A field contained a value outside its accepted grammar.
    #[error("invalid value `{value}` for PHQA field `{field}`")]
    InvalidValue {
        /// Field whose value was invalid.
        field: &'static str,
        /// Rejected value.
        value: String,
    },
}

/// Parses one complete serial line.
///
/// Non-PHQA diagnostic lines return `Ok(None)`. Lines beginning with the PHQA
/// prefix are parsed strictly and never silently ignored.
pub fn parse_line(line: &[u8]) -> Result<Option<Record>, ProtocolError> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.len() > MAX_LINE_LEN {
        return Err(ProtocolError::LineTooLong);
    }
    if !line.starts_with(b"PHQA|") {
        return Ok(None);
    }

    let text = std::str::from_utf8(line)
        .map_err(|_| ProtocolError::InvalidField("non-ASCII protocol line".to_owned()))?;
    if !text.is_ascii() {
        return Err(ProtocolError::InvalidField(
            "non-ASCII protocol line".to_owned(),
        ));
    }

    let mut parts = text.split('|');
    let prefix = parts.next().unwrap_or_default();
    debug_assert_eq!(prefix, "PHQA");
    let version = parts.next().ok_or(ProtocolError::MissingField("version"))?;
    if version != "1" {
        return Err(ProtocolError::UnsupportedVersion(version.to_owned()));
    }
    let kind = parts.next().ok_or(ProtocolError::MissingField("kind"))?;

    let mut fields = BTreeMap::new();
    for field in parts {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| ProtocolError::InvalidField(field.to_owned()))?;
        if !valid_key(key) || !valid_token(value) {
            return Err(ProtocolError::InvalidField(field.to_owned()));
        }
        if fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(ProtocolError::DuplicateField(key.to_owned()));
        }
    }

    let record = match kind {
        "RUN_START" => Record::RunStart {
            run_id: take_run_id(&mut fields)?,
            suite: take_token(&mut fields, "suite")?,
            commit: take_commit(&mut fields)?,
            mode: take_mode(&mut fields)?,
            reset_reason: take_token(&mut fields, "reset_reason")?,
        },
        "READY" => Record::Ready {
            run_id: take_run_id(&mut fields)?,
            step: take_u32(&mut fields, "step")?,
            action: take_token(&mut fields, "action")?,
        },
        "OBS" => Record::Observation {
            run_id: take_run_id(&mut fields)?,
            name: take_token(&mut fields, "name")?,
            value: take_token(&mut fields, "value")?,
        },
        "TEST" => {
            let run_id = take_run_id(&mut fields)?;
            let id = take_token(&mut fields, "id")?;
            let status = take_status(&mut fields)?;
            let required = take_required(&mut fields)?;
            let reason = fields.remove("reason");
            if status != TestStatus::Pass && reason.is_none() {
                return Err(ProtocolError::MissingField("reason"));
            }
            if status == TestStatus::Pass && reason.is_some() {
                return Err(ProtocolError::UnexpectedField("reason".to_owned()));
            }
            Record::Test {
                run_id,
                id,
                status,
                required,
                reason,
            }
        }
        "RUN_END" => Record::RunEnd {
            run_id: take_run_id(&mut fields)?,
            result: take_result(&mut fields)?,
        },
        other => return Err(ProtocolError::UnknownKind(other.to_owned())),
    };

    if let Some(field) = fields.keys().next() {
        return Err(ProtocolError::UnexpectedField(field.clone()));
    }
    Ok(Some(record))
}

fn valid_key(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            matches!(
                byte,
                b'a'..=b'z'
                    | b'A'..=b'Z'
                    | b'0'..=b'9'
                    | b'_'
                    | b'-'
                    | b'.'
                    | b':'
                    | b'/'
                    | b'+'
            )
        })
}

fn take_token(
    fields: &mut BTreeMap<String, String>,
    key: &'static str,
) -> Result<String, ProtocolError> {
    fields.remove(key).ok_or(ProtocolError::MissingField(key))
}

fn take_run_id(fields: &mut BTreeMap<String, String>) -> Result<u64, ProtocolError> {
    let value = take_token(fields, "run")?;
    if value.len() > 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProtocolError::InvalidValue {
            field: "run",
            value,
        });
    }
    u64::from_str_radix(&value, 16).map_err(|_| ProtocolError::InvalidValue {
        field: "run",
        value,
    })
}

fn take_commit(fields: &mut BTreeMap<String, String>) -> Result<String, ProtocolError> {
    let value = take_token(fields, "commit")?;
    if !(7..=64).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ProtocolError::InvalidValue {
            field: "commit",
            value,
        });
    }
    Ok(value)
}

fn take_u32(
    fields: &mut BTreeMap<String, String>,
    key: &'static str,
) -> Result<u32, ProtocolError> {
    let value = take_token(fields, key)?;
    value
        .parse()
        .map_err(|_| ProtocolError::InvalidValue { field: key, value })
}

fn take_mode(fields: &mut BTreeMap<String, String>) -> Result<RunMode, ProtocolError> {
    let value = take_token(fields, "mode")?;
    match value.as_str() {
        "exploratory" => Ok(RunMode::Exploratory),
        "release" => Ok(RunMode::Release),
        _ => Err(ProtocolError::InvalidValue {
            field: "mode",
            value,
        }),
    }
}

fn take_status(fields: &mut BTreeMap<String, String>) -> Result<TestStatus, ProtocolError> {
    let value = take_token(fields, "status")?;
    match value.as_str() {
        "PASS" => Ok(TestStatus::Pass),
        "FAIL" => Ok(TestStatus::Fail),
        "SKIP" => Ok(TestStatus::Skip),
        "BLOCKED" => Ok(TestStatus::Blocked),
        _ => Err(ProtocolError::InvalidValue {
            field: "status",
            value,
        }),
    }
}

fn take_required(fields: &mut BTreeMap<String, String>) -> Result<bool, ProtocolError> {
    let value = take_token(fields, "required")?;
    match value.as_str() {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(ProtocolError::InvalidValue {
            field: "required",
            value,
        }),
    }
}

fn take_result(fields: &mut BTreeMap<String, String>) -> Result<RunResult, ProtocolError> {
    let value = take_token(fields, "result")?;
    match value.as_str() {
        "PASS" => Ok(RunResult::Pass),
        "FAIL" => Ok(RunResult::Fail),
        _ => Err(ProtocolError::InvalidValue {
            field: "result",
            value,
        }),
    }
}

/// Converts arbitrary serial chunks into complete, bounded lines.
#[derive(Debug, Default)]
pub struct LineDecoder {
    buffered: Vec<u8>,
}

impl LineDecoder {
    /// Constructs an empty line decoder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffered: Vec::new(),
        }
    }

    /// Adds a serial chunk and returns each newly completed line.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, ProtocolError> {
        let mut lines = Vec::new();
        for byte in chunk {
            if *byte == b'\n' {
                let mut line = std::mem::take(&mut self.buffered);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                lines.push(line);
            } else {
                self.buffered.push(*byte);
                if self.buffered.len() > MAX_LINE_LEN {
                    return Err(ProtocolError::LineTooLong);
                }
            }
        }
        Ok(lines)
    }

    /// Verifies that the stream ended on a line boundary.
    pub fn finish(self) -> Result<(), ProtocolError> {
        if self.buffered.is_empty() {
            Ok(())
        } else {
            Err(ProtocolError::TruncatedLine)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LineDecoder, ProtocolError, Record, RunMode, RunResult, TestStatus, parse_line};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn parses_all_record_kinds() {
        assert_eq!(
            parse_line(
                format!(
                    "PHQA|1|RUN_START|run=abc|suite=mdio|commit={COMMIT}|mode=release|reset_reason=power_on"
                )
                .as_bytes()
            ),
            Ok(Some(Record::RunStart {
                run_id: 0xabc,
                suite: "mdio".to_owned(),
                commit: COMMIT.to_owned(),
                mode: RunMode::Release,
                reset_reason: "power_on".to_owned(),
            }))
        );
        assert_eq!(
            parse_line(b"PHQA|1|READY|run=abc|step=2|action=link_down"),
            Ok(Some(Record::Ready {
                run_id: 0xabc,
                step: 2,
                action: "link_down".to_owned(),
            }))
        );
        assert_eq!(
            parse_line(b"PHQA|1|OBS|run=abc|name=phy_id|value=0007:c0f0"),
            Ok(Some(Record::Observation {
                run_id: 0xabc,
                name: "phy_id".to_owned(),
                value: "0007:c0f0".to_owned(),
            }))
        );
        assert_eq!(
            parse_line(b"PHQA|1|TEST|run=abc|id=mdio.phy-id|status=PASS|required=1"),
            Ok(Some(Record::Test {
                run_id: 0xabc,
                id: "mdio.phy-id".to_owned(),
                status: TestStatus::Pass,
                required: true,
                reason: None,
            }))
        );
        assert_eq!(
            parse_line(b"PHQA|1|RUN_END|run=abc|result=PASS"),
            Ok(Some(Record::RunEnd {
                run_id: 0xabc,
                result: RunResult::Pass,
            }))
        );
    }

    #[test]
    fn ignores_diagnostic_lines() {
        assert_eq!(parse_line(b"[INFO] board booted"), Ok(None));
    }

    #[test]
    fn rejects_duplicate_fields() {
        assert_eq!(
            parse_line(b"PHQA|1|RUN_END|run=1|run=2|result=PASS"),
            Err(ProtocolError::DuplicateField("run".to_owned()))
        );
    }

    #[test]
    fn requires_reason_for_non_pass() {
        assert_eq!(
            parse_line(b"PHQA|1|TEST|run=1|id=x|status=FAIL|required=1"),
            Err(ProtocolError::MissingField("reason"))
        );
    }

    #[test]
    fn rejects_reason_for_pass() {
        assert_eq!(
            parse_line(b"PHQA|1|TEST|run=1|id=x|status=PASS|required=1|reason=none"),
            Err(ProtocolError::UnexpectedField("reason".to_owned()))
        );
    }

    #[test]
    fn line_decoder_handles_chunk_and_crlf_boundaries() {
        let mut decoder = LineDecoder::new();
        assert!(decoder.push(b"PHQA|1|RUN_").unwrap().is_empty());
        assert_eq!(
            decoder.push(b"END|run=1|result=PASS\r\nnoise\n").unwrap(),
            vec![
                b"PHQA|1|RUN_END|run=1|result=PASS".to_vec(),
                b"noise".to_vec()
            ]
        );
        assert_eq!(decoder.finish(), Ok(()));
    }

    #[test]
    fn line_decoder_rejects_truncation() {
        let mut decoder = LineDecoder::new();
        decoder.push(b"PHQA|1|RUN_").unwrap();
        assert_eq!(decoder.finish(), Err(ProtocolError::TruncatedLine));
    }

    #[test]
    fn line_decoder_rejects_oversize_input() {
        let mut decoder = LineDecoder::new();
        assert_eq!(
            decoder.push(&vec![b'x'; super::MAX_LINE_LEN + 1]),
            Err(ProtocolError::LineTooLong)
        );
    }
}
