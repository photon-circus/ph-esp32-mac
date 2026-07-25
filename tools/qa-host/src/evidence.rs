//! Reproducible evidence artifacts for successful and failed QA runs.

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{OffsetDateTime, macros::format_description};

use crate::{
    protocol::{RunResult, TestStatus},
    session::SessionReport,
    suite::Suite,
};

const EVIDENCE_SCHEMA: u32 = 1;
const DIRECTORY_FORMAT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year][month][day]T[hour][minute][second].[subsecond digits:3]Z");
const RFC3339_FORMAT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");
const MINIMAL_PCAP_HEADER: [u8; 24] = [
    0xd4, 0xc3, 0xb2, 0xa1, // little-endian microsecond magic
    0x02, 0x00, 0x04, 0x00, // version 2.4
    0x00, 0x00, 0x00, 0x00, // timezone correction
    0x00, 0x00, 0x00, 0x00, // timestamp accuracy
    0xff, 0xff, 0x00, 0x00, // snap length 65535
    0x01, 0x00, 0x00, 0x00, // Ethernet link type
];

/// Evidence trust grade.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceGrade {
    /// Exact clean-tree evidence suitable for a release gate.
    Release,
    /// Dirty-tree evidence retained only for candidate-branch investigation.
    Sandbox,
}

/// One failed harness condition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FailureRecord {
    /// Stable failure category.
    pub category: String,
    /// Human-readable detail.
    pub message: String,
}

/// One external action invocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionRecord {
    /// Firmware or matrix action name.
    pub action: String,
    /// Executable basename, without potentially sensitive arguments.
    pub executable: String,
    /// Child-process exit code when one was available.
    pub exit_code: Option<i32>,
    /// Wall-clock duration.
    pub duration_ms: u128,
    /// Whether the action completed successfully.
    pub success: bool,
    /// Suite identity encoded into a packet action.
    pub suite: Option<Suite>,
    /// Lowercase hexadecimal run identity encoded into a packet action.
    pub run_id: Option<String>,
    /// READY step encoded into a packet action.
    pub step: Option<u32>,
    /// Frames requested or observed for packet actions.
    pub count: Option<u64>,
}

/// SHA-256 record for a repository lockfile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LockHash {
    /// Repository-relative lockfile path.
    pub path: String,
    /// Lowercase SHA-256 digest.
    pub sha256: String,
}

/// Captured external tool version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolVersion {
    /// Tool identifier.
    pub name: String,
    /// First normalized version-output line.
    pub version: String,
}

/// Aggregate normalized run data written to `result.json`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NormalizedResult {
    /// Evidence schema version.
    pub schema: u32,
    /// Requested suite.
    pub suite: Suite,
    /// Final host-computed result.
    pub result: RunResult,
    /// Complete firmware sessions, including matrix iterations.
    pub sessions: Vec<SessionReport>,
    /// Harness and protocol failures not represented by a firmware TEST.
    pub failures: Vec<FailureRecord>,
}

impl NormalizedResult {
    /// Constructs an aggregate and derives its final result.
    #[must_use]
    pub fn new(suite: Suite, sessions: Vec<SessionReport>, failures: Vec<FailureRecord>) -> Self {
        let passed = failures.is_empty()
            && !sessions.is_empty()
            && sessions.iter().all(SessionReport::passed);
        Self {
            schema: EVIDENCE_SCHEMA,
            suite,
            result: if passed {
                RunResult::Pass
            } else {
                RunResult::Fail
            },
            sessions,
            failures,
        }
    }

    /// Returns true only for a complete passing aggregate.
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self.result, RunResult::Pass)
    }
}

/// Inputs needed to finalize an evidence directory.
pub struct FinalizeEvidence {
    /// Normalized aggregate.
    pub result: NormalizedResult,
    /// External lab actions.
    pub actions: Vec<ActionRecord>,
    /// Repository lockfile hashes.
    pub lockfiles: Vec<LockHash>,
    /// Tool versions.
    pub tools: Vec<ToolVersion>,
}

#[derive(Serialize)]
struct Metadata<'a> {
    schema: u32,
    suite: Suite,
    commit: &'a str,
    dirty: bool,
    grade: EvidenceGrade,
    board_id: &'a str,
    started_utc: &'a str,
    finished_utc: &'a str,
    result: RunResult,
}

#[derive(Serialize)]
struct ResetReason<'a> {
    index: usize,
    run_id: &'a str,
    reset_reason: &'a str,
}

/// A partially written evidence directory.
///
/// `INCOMPLETE` remains present unless every final artifact and checksum was
/// written successfully.
pub struct EvidenceBundle {
    root: PathBuf,
    suite: Suite,
    commit: String,
    dirty: bool,
    grade: EvidenceGrade,
    board_id: String,
    started_utc: String,
    serial: File,
}

impl EvidenceBundle {
    /// Creates a uniquely named evidence directory under `base`.
    pub fn create(
        base: impl AsRef<Path>,
        suite: Suite,
        commit: impl Into<String>,
        dirty: bool,
        board_id: impl Into<String>,
    ) -> Result<Self, EvidenceError> {
        Self::create_at(
            base.as_ref(),
            suite,
            commit.into(),
            dirty,
            board_id.into(),
            OffsetDateTime::now_utc(),
        )
    }

    fn create_at(
        base: &Path,
        suite: Suite,
        commit: String,
        dirty: bool,
        board_id: String,
        started: OffsetDateTime,
    ) -> Result<Self, EvidenceError> {
        let timestamp = started
            .format(DIRECTORY_FORMAT)
            .map_err(EvidenceError::Time)?;
        let short_commit: String = commit.chars().take(8).collect();
        let prefix = format!("{timestamp}-{short_commit}");
        fs::create_dir_all(base).map_err(|source| EvidenceError::Io {
            path: base.to_path_buf(),
            source,
        })?;

        let mut root = base.join(&prefix);
        let mut suffix = 0_u32;
        while root.exists() {
            suffix += 1;
            root = base.join(format!("{prefix}-{suffix:02}"));
        }
        fs::create_dir(&root).map_err(|source| EvidenceError::Io {
            path: root.clone(),
            source,
        })?;
        write_bytes(root.join("INCOMPLETE"), b"evidence finalization pending\n")?;
        write_bytes(root.join("traffic.pcap"), &MINIMAL_PCAP_HEADER)?;
        File::create(root.join("build.log")).map_err(|source| EvidenceError::Io {
            path: root.join("build.log"),
            source,
        })?;
        let serial_path = root.join("serial.log");
        let serial = File::create(&serial_path).map_err(|source| EvidenceError::Io {
            path: serial_path,
            source,
        })?;
        let started_utc = started
            .format(RFC3339_FORMAT)
            .map_err(EvidenceError::Time)?;

        Ok(Self {
            root,
            suite,
            commit,
            dirty,
            grade: if dirty {
                EvidenceGrade::Sandbox
            } else {
                EvidenceGrade::Release
            },
            board_id,
            started_utc,
            serial,
        })
    }

    /// Returns the evidence directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the packet-capture path for the active adapter.
    #[must_use]
    pub fn packet_capture_path(&self) -> PathBuf {
        self.root.join("traffic.pcap")
    }

    /// Appends exact UART bytes before any decoding.
    pub fn append_serial(&mut self, bytes: &[u8]) -> Result<(), EvidenceError> {
        self.serial
            .write_all(bytes)
            .map_err(|source| EvidenceError::Io {
                path: self.root.join("serial.log"),
                source,
            })?;
        self.serial.flush().map_err(|source| EvidenceError::Io {
            path: self.root.join("serial.log"),
            source,
        })
    }

    /// Replaces the captured build log.
    pub fn write_build_log(&self, bytes: &[u8]) -> Result<(), EvidenceError> {
        write_bytes(self.root.join("build.log"), bytes)
    }

    /// Appends another independently compiled firmware transcript.
    pub fn append_build_log(&self, bytes: &[u8]) -> Result<(), EvidenceError> {
        let path = self.root.join("build.log");
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|source| EvidenceError::Io {
                path: path.clone(),
                source,
            })?;
        file.write_all(bytes)
            .map_err(|source| EvidenceError::Io { path, source })
    }

    /// Copies and hashes the exact ELF flashed by the harness.
    pub fn copy_firmware(&self, firmware: &Path) -> Result<String, EvidenceError> {
        self.copy_firmware_named(firmware, self.suite.binary_name())
    }

    /// Copies and hashes an ELF under a stable evidence-local label.
    pub fn copy_firmware_named(
        &self,
        firmware: &Path,
        label: &str,
    ) -> Result<String, EvidenceError> {
        let firmware_dir = self.root.join("firmware");
        fs::create_dir_all(&firmware_dir).map_err(|source| EvidenceError::Io {
            path: firmware_dir.clone(),
            source,
        })?;
        if label.is_empty()
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(EvidenceError::InvalidFirmwareLabel(label.to_owned()));
        }
        let destination = firmware_dir.join(format!("{label}.elf"));
        fs::copy(firmware, &destination).map_err(|source| EvidenceError::Io {
            path: destination.clone(),
            source,
        })?;
        let hash = hash_file(&destination)?;
        write_firmware_hashes(&self.root, &firmware_dir)?;
        Ok(hash)
    }

    /// Writes all normalized artifacts, checksums them, and removes
    /// `INCOMPLETE`.
    pub fn finalize(mut self, data: FinalizeEvidence) -> Result<PathBuf, EvidenceError> {
        self.serial.flush().map_err(|source| EvidenceError::Io {
            path: self.root.join("serial.log"),
            source,
        })?;
        drop(self.serial);

        let finished_utc = OffsetDateTime::now_utc()
            .format(RFC3339_FORMAT)
            .map_err(EvidenceError::Time)?;
        let metadata = Metadata {
            schema: EVIDENCE_SCHEMA,
            suite: self.suite,
            commit: &self.commit,
            dirty: self.dirty,
            grade: self.grade,
            board_id: &self.board_id,
            started_utc: &self.started_utc,
            finished_utc: &finished_utc,
            result: data.result.result,
        };
        write_json(self.root.join("metadata.json"), &metadata)?;
        write_json(self.root.join("result.json"), &data.result)?;
        write_json(self.root.join("lockfiles.json"), &data.lockfiles)?;
        write_json(self.root.join("tools.json"), &data.tools)?;

        let reset_reasons: Vec<_> = data
            .result
            .sessions
            .iter()
            .enumerate()
            .map(|(index, session)| ResetReason {
                index,
                run_id: &session.run_id,
                reset_reason: &session.reset_reason,
            })
            .collect();
        write_json(self.root.join("reset-reasons.json"), &reset_reasons)?;
        write_action_log(self.root.join("actions.jsonl"), &data.actions)?;
        write_bytes(
            self.root.join("junit.xml"),
            junit_xml(&data.result).as_bytes(),
        )?;
        write_bytes(
            self.root.join("SUMMARY.md"),
            summary_markdown(&data.result, self.grade).as_bytes(),
        )?;
        write_sha256sums(&self.root)?;

        let incomplete = self.root.join("INCOMPLETE");
        fs::remove_file(&incomplete).map_err(|source| EvidenceError::Io {
            path: incomplete,
            source,
        })?;
        Ok(self.root)
    }
}

/// Collects SHA-256 hashes for every repository lockfile relevant to QA.
pub fn collect_lock_hashes(repo: &Path) -> Result<Vec<LockHash>, EvidenceError> {
    let candidates = [
        "Cargo.lock",
        "apps/examples/Cargo.lock",
        "apps/qa-runner/Cargo.lock",
        "xtask/Cargo.lock",
        "tools/qa-host/Cargo.lock",
    ];
    candidates
        .into_iter()
        .filter(|relative| repo.join(relative).is_file())
        .map(|relative| {
            Ok(LockHash {
                path: relative.to_owned(),
                sha256: hash_file(&repo.join(relative))?,
            })
        })
        .collect()
}

/// Computes a lowercase SHA-256 digest for one file.
pub fn hash_file(path: &Path) -> Result<String, EvidenceError> {
    let mut file = File::open(path).map_err(|source| EvidenceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1_024];
    loop {
        let read = file.read(&mut buffer).map_err(|source| EvidenceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_firmware_hashes(root: &Path, firmware_dir: &Path) -> Result<(), EvidenceError> {
    let mut entries = fs::read_dir(firmware_dir)
        .map_err(|source| EvidenceError::Io {
            path: firmware_dir.to_path_buf(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| EvidenceError::Io {
            path: firmware_dir.to_path_buf(),
            source,
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let mut index = String::new();
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("elf") {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        index.push_str(&hash_file(&path)?);
        index.push_str("  firmware/");
        index.push_str(&name);
        index.push('\n');
    }
    write_bytes(root.join("firmware.sha256"), index.as_bytes())
}

fn write_json(path: PathBuf, value: &impl Serialize) -> Result<(), EvidenceError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(EvidenceError::Json)?;
    bytes.push(b'\n');
    write_bytes(path, &bytes)
}

fn write_action_log(path: PathBuf, actions: &[ActionRecord]) -> Result<(), EvidenceError> {
    let mut file = File::create(&path).map_err(|source| EvidenceError::Io {
        path: path.clone(),
        source,
    })?;
    for action in actions {
        serde_json::to_writer(&mut file, action).map_err(EvidenceError::Json)?;
        file.write_all(b"\n").map_err(|source| EvidenceError::Io {
            path: path.clone(),
            source,
        })?;
    }
    Ok(())
}

fn write_bytes(path: PathBuf, bytes: &[u8]) -> Result<(), EvidenceError> {
    fs::write(&path, bytes).map_err(|source| EvidenceError::Io { path, source })
}

fn write_sha256sums(root: &Path) -> Result<(), EvidenceError> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut output = String::new();
    for (relative, absolute) in files {
        if relative == "SHA256SUMS" || relative == "INCOMPLETE" {
            continue;
        }
        output.push_str(&hash_file(&absolute)?);
        output.push_str("  ");
        output.push_str(&relative);
        output.push('\n');
    }
    write_bytes(root.join("SHA256SUMS"), output.as_bytes())
}

fn collect_files(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), EvidenceError> {
    for entry in fs::read_dir(directory).map_err(|source| EvidenceError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| EvidenceError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, output)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            output.push((relative, path));
        }
    }
    Ok(())
}

fn junit_xml(result: &NormalizedResult) -> String {
    let firmware_tests = result
        .sessions
        .iter()
        .map(|session| session.tests.len())
        .sum::<usize>();
    let test_count = firmware_tests + result.failures.len();
    let failures = result
        .sessions
        .iter()
        .flat_map(|session| &session.tests)
        .filter(|test| {
            test.status == TestStatus::Fail
                || (test.required && matches!(test.status, TestStatus::Skip | TestStatus::Blocked))
        })
        .count()
        + result.failures.len();
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"phqa.{}\" tests=\"{test_count}\" failures=\"{failures}\">\n",
        result.suite
    );
    for (session_index, session) in result.sessions.iter().enumerate() {
        for test in &session.tests {
            xml.push_str(&format!(
                "  <testcase classname=\"phqa.{}.session-{session_index}\" name=\"{}\">",
                result.suite,
                xml_escape(&test.id)
            ));
            match test.status {
                TestStatus::Pass => {}
                TestStatus::Skip if !test.required => xml.push_str("<skipped/>"),
                status => {
                    let reason = test.reason.as_deref().unwrap_or("unspecified");
                    xml.push_str(&format!(
                        "<failure message=\"{}\">{:?}</failure>",
                        xml_escape(reason),
                        status
                    ));
                }
            }
            xml.push_str("</testcase>\n");
        }
    }
    for failure in &result.failures {
        xml.push_str(&format!(
            "  <testcase classname=\"phqa.harness\" name=\"{}\"><failure message=\"{}\">{}</failure></testcase>\n",
            xml_escape(&failure.category),
            xml_escape(&failure.message),
            xml_escape(&failure.message)
        ));
    }
    xml.push_str("</testsuite>\n");
    xml
}

fn summary_markdown(result: &NormalizedResult, grade: EvidenceGrade) -> String {
    let status = if result.passed() { "PASS" } else { "FAIL" };
    let tests = result
        .sessions
        .iter()
        .map(|session| session.tests.len())
        .sum::<usize>();
    format!(
        "# QA Evidence Summary\n\n- Suite: `{}`\n- Result: **{status}**\n- Grade: `{:?}`\n- Sessions: {}\n- Firmware tests: {tests}\n- Harness failures: {}\n",
        result.suite,
        grade,
        result.sessions.len(),
        result.failures.len()
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Evidence generation failure.
#[derive(Debug, Error)]
pub enum EvidenceError {
    /// An evidence-local firmware label was unsafe or ambiguous.
    #[error("invalid firmware evidence label `{0}`")]
    InvalidFirmwareLabel(String),
    /// Filesystem operation failed.
    #[error("evidence I/O failed for {}: {source}", path.display())]
    Io {
        /// Affected path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// JSON serialization failed.
    #[error("evidence JSON serialization failed: {0}")]
    Json(serde_json::Error),
    /// UTC timestamp formatting failed.
    #[error("evidence timestamp formatting failed: {0}")]
    Time(time::error::Format),
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;
    use time::macros::datetime;

    use crate::{
        protocol::{RunResult, TestStatus},
        session::{SessionReport, TestOutcome},
        suite::Suite,
    };

    use super::{
        EvidenceBundle, EvidenceGrade, FailureRecord, FinalizeEvidence, NormalizedResult, hash_file,
    };

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn report(status: TestStatus, result: RunResult) -> SessionReport {
        SessionReport {
            run_id: "1234".to_owned(),
            suite: Suite::Mdio,
            commit: COMMIT.to_owned(),
            reset_reason: "power_on".to_owned(),
            ready: Vec::new(),
            observations: Vec::new(),
            tests: vec![TestOutcome {
                id: "mdio.read-repeat".to_owned(),
                status,
                required: true,
                reason: (status != TestStatus::Pass).then(|| "no_link".to_owned()),
            }],
            result,
        }
    }

    fn finalize(root: &Path, result: NormalizedResult) -> std::path::PathBuf {
        let mut bundle = EvidenceBundle::create_at(
            root,
            Suite::Mdio,
            COMMIT.to_owned(),
            false,
            "board-a".to_owned(),
            datetime!(2026-07-25 12:34:56.123 UTC),
        )
        .unwrap();
        bundle.append_serial(b"PHQA raw\r\n").unwrap();
        bundle
            .finalize(FinalizeEvidence {
                result,
                actions: Vec::new(),
                lockfiles: Vec::new(),
                tools: Vec::new(),
            })
            .unwrap()
    }

    #[test]
    fn finalizes_complete_artifact_set_and_checksums() {
        let temp = tempdir().unwrap();
        let result = NormalizedResult::new(
            Suite::Mdio,
            vec![report(TestStatus::Pass, RunResult::Pass)],
            Vec::new(),
        );
        let root = finalize(temp.path(), result);
        for file in [
            "actions.jsonl",
            "build.log",
            "junit.xml",
            "lockfiles.json",
            "metadata.json",
            "reset-reasons.json",
            "result.json",
            "serial.log",
            "SUMMARY.md",
            "tools.json",
            "traffic.pcap",
            "SHA256SUMS",
        ] {
            assert!(root.join(file).is_file(), "missing {file}");
        }
        assert!(!root.join("INCOMPLETE").exists());
        let sums = fs::read_to_string(root.join("SHA256SUMS")).unwrap();
        assert!(!sums.contains("SHA256SUMS"));
        assert!(!sums.contains("INCOMPLETE"));
        let paths: Vec<_> = sums
            .lines()
            .map(|line| line.split_once("  ").unwrap().1)
            .collect();
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        assert_eq!(paths, sorted);
        assert_eq!(fs::read(root.join("serial.log")).unwrap(), b"PHQA raw\r\n");
    }

    #[test]
    fn failures_are_written_to_json_and_junit() {
        let temp = tempdir().unwrap();
        let result = NormalizedResult::new(
            Suite::Mdio,
            vec![report(TestStatus::Fail, RunResult::Fail)],
            vec![FailureRecord {
                category: "protocol<&".to_owned(),
                message: "bad \"line\"".to_owned(),
            }],
        );
        let root = finalize(temp.path(), result);
        let junit = fs::read_to_string(root.join("junit.xml")).unwrap();
        assert!(junit.contains("protocol&lt;&amp;"));
        assert!(junit.contains("bad &quot;line&quot;"));
        let json = fs::read_to_string(root.join("result.json")).unwrap();
        assert!(json.contains("\"result\": \"FAIL\""));
    }

    #[test]
    fn dirty_runs_are_marked_sandbox() {
        let temp = tempdir().unwrap();
        let bundle = EvidenceBundle::create_at(
            temp.path(),
            Suite::Mdio,
            COMMIT.to_owned(),
            true,
            "board-a".to_owned(),
            datetime!(2026-07-25 12:34:56.123 UTC),
        )
        .unwrap();
        assert_eq!(bundle.grade, EvidenceGrade::Sandbox);
        assert!(bundle.root().join("INCOMPLETE").is_file());
    }

    #[test]
    fn hashes_files_deterministically() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("sample");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            hash_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
