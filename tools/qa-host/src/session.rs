//! Deterministic validation state machine for one firmware run.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    protocol::{Record, RunMode, RunResult, TestStatus},
    suite::Suite,
};

/// One host action requested by firmware.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadyEvent {
    /// Monotonic action step.
    pub step: u32,
    /// Code-owned action name.
    pub action: String,
}

/// One named observation emitted by firmware.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Observation {
    /// Observation name.
    pub name: String,
    /// Token-form value.
    pub value: String,
}

/// One normalized test result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TestOutcome {
    /// Stable test ID.
    pub id: String,
    /// Firmware-observed status.
    pub status: TestStatus,
    /// Whether a skipped or blocked result invalidates the run.
    pub required: bool,
    /// Machine-readable failure reason, when present.
    pub reason: Option<String>,
}

/// Normalized report for a complete firmware run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionReport {
    /// Protocol run identifier.
    pub run_id: String,
    /// Firmware suite.
    pub suite: Suite,
    /// Source commit embedded in firmware.
    pub commit: String,
    /// Reset reason captured at boot.
    pub reset_reason: String,
    /// Host synchronization points.
    pub ready: Vec<ReadyEvent>,
    /// Ordered hardware observations.
    pub observations: Vec<Observation>,
    /// Test outcomes ordered by firmware emission.
    pub tests: Vec<TestOutcome>,
    /// Aggregate result computed by the host.
    pub result: RunResult,
}

impl SessionReport {
    /// Returns true only when the host computed a passing result.
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self.result, RunResult::Pass)
    }

    /// Returns true when firmware requested an action during this run.
    #[must_use]
    pub fn requested(&self, action: &str) -> bool {
        self.ready.iter().any(|ready| ready.action == action)
    }
}

/// Event returned while consuming a record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    /// No immediate host work is required.
    Continue,
    /// Firmware is waiting for an external action.
    Ready(ReadyEvent),
    /// Firmware emitted a valid terminal record.
    Complete(SessionReport),
}

/// Strict session-validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SessionError {
    /// A record other than `RUN_START` occurred first.
    #[error("record occurred before RUN_START")]
    BeforeStart,
    /// Input ended without any run.
    #[error("serial stream ended before RUN_START")]
    MissingStart,
    /// Input ended before `RUN_END`.
    #[error("serial stream ended before RUN_END")]
    MissingTerminal,
    /// Firmware restarted before completing the current run.
    #[error("unexpected reset before RUN_END")]
    UnexpectedReset,
    /// A record occurred after the terminal record.
    #[error("record occurred after RUN_END")]
    AfterTerminal,
    /// A record was buffered from a different run.
    #[error("stale run ID: expected {expected:x}, received {actual:x}")]
    StaleRun {
        /// Active run identifier.
        expected: u64,
        /// Rejected run identifier.
        actual: u64,
    },
    /// Firmware was built from a different source revision.
    #[error("stale firmware commit: expected {expected}, received {actual}")]
    StaleCommit {
        /// Checked-out source revision.
        expected: String,
        /// Revision embedded in firmware.
        actual: String,
    },
    /// Firmware announced a different suite.
    #[error("suite mismatch: expected {expected}, received {actual}")]
    SuiteMismatch {
        /// Requested suite.
        expected: Suite,
        /// Firmware suite token.
        actual: String,
    },
    /// A release harness received exploratory firmware.
    #[error("firmware is not in release mode")]
    ExploratoryFirmware,
    /// Release firmware used the reserved fallback run identifier.
    #[error("release firmware used fallback run ID 0")]
    FallbackRunId,
    /// READY steps were duplicated or out of sequence.
    #[error("READY step mismatch: expected {expected}, received {actual}")]
    ReadyStep {
        /// Next required step.
        expected: u32,
        /// Rejected step.
        actual: u32,
    },
    /// Firmware requested an action outside the suite contract.
    #[error("action `{0}` is not permitted for this suite")]
    UnexpectedAction(String),
    /// A test ID occurred more than once.
    #[error("duplicate test ID `{0}`")]
    DuplicateTest(String),
    /// A test ID was not part of the host-owned suite.
    #[error("unknown test ID `{0}`")]
    UnknownTest(String),
    /// Firmware attempted to downgrade a release-required test.
    #[error("test `{0}` must declare required=1")]
    RequiredFlagMismatch(String),
    /// One or more expected test IDs were missing at `RUN_END`.
    #[error("missing required test IDs: {0:?}")]
    MissingTests(Vec<String>),
    /// Firmware's terminal result contradicted host computation.
    #[error("RUN_END result mismatch: expected {expected:?}, received {actual:?}")]
    ResultMismatch {
        /// Result computed from individual tests.
        expected: RunResult,
        /// Result claimed by firmware.
        actual: RunResult,
    },
}

enum State {
    Waiting,
    Running(Running),
    Complete(SessionReport),
}

struct Running {
    run_id: u64,
    commit: String,
    reset_reason: String,
    ready: Vec<ReadyEvent>,
    observations: Vec<Observation>,
    tests: Vec<TestOutcome>,
    seen_tests: BTreeSet<String>,
    failed: bool,
}

/// Validates ordering, identity, completeness, and aggregate results.
pub struct SessionValidator {
    suite: Suite,
    expected_commit: String,
    expected_run_id: Option<u64>,
    require_release: bool,
    state: State,
}

impl SessionValidator {
    /// Constructs a strict validator for one suite and source revision.
    #[must_use]
    pub fn new(suite: Suite, expected_commit: impl Into<String>, require_release: bool) -> Self {
        Self {
            suite,
            expected_commit: expected_commit.into(),
            expected_run_id: None,
            require_release,
            state: State::Waiting,
        }
    }

    /// Requires `RUN_START` to match the ID compiled into the flashed image.
    #[must_use]
    pub const fn expect_run_id(mut self, run_id: u64) -> Self {
        self.expected_run_id = Some(run_id);
        self
    }

    /// Consumes one parsed record and returns any required host action.
    pub fn consume(&mut self, record: Record) -> Result<SessionEvent, SessionError> {
        match &mut self.state {
            State::Waiting => self.start(record),
            State::Running(running) => {
                if matches!(record, Record::RunStart { .. }) {
                    return Err(SessionError::UnexpectedReset);
                }
                if record.run_id() != running.run_id {
                    return Err(SessionError::StaleRun {
                        expected: running.run_id,
                        actual: record.run_id(),
                    });
                }
                self.consume_running(record)
            }
            State::Complete(_) => Err(SessionError::AfterTerminal),
        }
    }

    /// Finishes the input stream and returns its complete report.
    pub fn finish(self) -> Result<SessionReport, SessionError> {
        match self.state {
            State::Waiting => Err(SessionError::MissingStart),
            State::Running(_) => Err(SessionError::MissingTerminal),
            State::Complete(report) => Ok(report),
        }
    }

    fn start(&mut self, record: Record) -> Result<SessionEvent, SessionError> {
        let Record::RunStart {
            run_id,
            suite,
            commit,
            mode,
            reset_reason,
        } = record
        else {
            return Err(SessionError::BeforeStart);
        };

        if suite != self.suite.to_string() {
            return Err(SessionError::SuiteMismatch {
                expected: self.suite,
                actual: suite,
            });
        }
        if commit != self.expected_commit {
            return Err(SessionError::StaleCommit {
                expected: self.expected_commit.clone(),
                actual: commit,
            });
        }
        if let Some(expected) = self.expected_run_id
            && run_id != expected
        {
            return Err(SessionError::StaleRun {
                expected,
                actual: run_id,
            });
        }
        if self.require_release && mode != RunMode::Release {
            return Err(SessionError::ExploratoryFirmware);
        }
        if self.require_release && run_id == 0 {
            return Err(SessionError::FallbackRunId);
        }

        self.state = State::Running(Running {
            run_id,
            commit,
            reset_reason,
            ready: Vec::new(),
            observations: Vec::new(),
            tests: Vec::new(),
            seen_tests: BTreeSet::new(),
            failed: false,
        });
        Ok(SessionEvent::Continue)
    }

    fn consume_running(&mut self, record: Record) -> Result<SessionEvent, SessionError> {
        let State::Running(running) = &mut self.state else {
            unreachable!("consume_running called outside running state");
        };

        match record {
            Record::RunStart { .. } => unreachable!("RUN_START handled before run-ID check"),
            Record::Ready { step, action, .. } => {
                let expected = u32::try_from(running.ready.len())
                    .unwrap_or(u32::MAX)
                    .saturating_add(1);
                if step != expected {
                    return Err(SessionError::ReadyStep {
                        expected,
                        actual: step,
                    });
                }
                if !self
                    .suite
                    .definition()
                    .allowed_actions
                    .contains(&action.as_str())
                {
                    return Err(SessionError::UnexpectedAction(action));
                }
                let ready = ReadyEvent { step, action };
                running.ready.push(ready.clone());
                Ok(SessionEvent::Ready(ready))
            }
            Record::Observation { name, value, .. } => {
                running.observations.push(Observation { name, value });
                Ok(SessionEvent::Continue)
            }
            Record::Test {
                id,
                status,
                required,
                reason,
                ..
            } => {
                if !self
                    .suite
                    .definition()
                    .required_tests
                    .contains(&id.as_str())
                {
                    return Err(SessionError::UnknownTest(id));
                }
                if !required {
                    return Err(SessionError::RequiredFlagMismatch(id));
                }
                if !running.seen_tests.insert(id.clone()) {
                    return Err(SessionError::DuplicateTest(id));
                }
                running.failed |= match status {
                    TestStatus::Pass => false,
                    TestStatus::Fail => true,
                    TestStatus::Skip | TestStatus::Blocked => required,
                };
                running.tests.push(TestOutcome {
                    id,
                    status,
                    required,
                    reason,
                });
                Ok(SessionEvent::Continue)
            }
            Record::RunEnd { result, .. } => {
                let missing: Vec<String> = self
                    .suite
                    .definition()
                    .required_tests
                    .iter()
                    .filter(|id| !running.seen_tests.contains(**id))
                    .map(|id| (*id).to_owned())
                    .collect();
                if !missing.is_empty() {
                    return Err(SessionError::MissingTests(missing));
                }

                let expected = if running.failed {
                    RunResult::Fail
                } else {
                    RunResult::Pass
                };
                if result != expected {
                    return Err(SessionError::ResultMismatch {
                        expected,
                        actual: result,
                    });
                }

                let report = SessionReport {
                    run_id: format!("{:x}", running.run_id),
                    suite: self.suite,
                    commit: running.commit.clone(),
                    reset_reason: running.reset_reason.clone(),
                    ready: std::mem::take(&mut running.ready),
                    observations: std::mem::take(&mut running.observations),
                    tests: std::mem::take(&mut running.tests),
                    result: expected,
                };
                self.state = State::Complete(report.clone());
                Ok(SessionEvent::Complete(report))
            }
        }
    }
}

/// Builds a map of test outcomes keyed by ID for report consumers.
#[must_use]
pub fn tests_by_id(report: &SessionReport) -> BTreeMap<&str, &TestOutcome> {
    report
        .tests
        .iter()
        .map(|test| (test.id.as_str(), test))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        protocol::{Record, RunMode, RunResult, TestStatus},
        suite::Suite,
    };

    use super::{SessionError, SessionEvent, SessionValidator};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const RUN: u64 = 0x1234;

    fn start(run_id: u64, commit: &str) -> Record {
        Record::RunStart {
            run_id,
            suite: "mdio".to_owned(),
            commit: commit.to_owned(),
            mode: RunMode::Release,
            reset_reason: "power_on".to_owned(),
        }
    }

    fn test(id: &str, status: TestStatus) -> Record {
        Record::Test {
            run_id: RUN,
            id: id.to_owned(),
            status,
            required: true,
            reason: (status != TestStatus::Pass).then(|| "observed".to_owned()),
        }
    }

    fn feed_all_tests(validator: &mut SessionValidator, status: TestStatus) {
        for id in Suite::Mdio.definition().required_tests {
            validator.consume(test(id, status)).unwrap();
        }
    }

    fn end(result: RunResult) -> Record {
        Record::RunEnd {
            run_id: RUN,
            result,
        }
    }

    #[test]
    fn accepts_complete_success() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        assert_eq!(
            validator.consume(start(RUN, COMMIT)),
            Ok(SessionEvent::Continue)
        );
        feed_all_tests(&mut validator, TestStatus::Pass);
        let SessionEvent::Complete(report) = validator.consume(end(RunResult::Pass)).unwrap()
        else {
            panic!("expected complete report");
        };
        assert!(report.passed());
        assert!(validator.finish().unwrap().passed());
    }

    #[test]
    fn explicit_failure_produces_failing_report() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        feed_all_tests(&mut validator, TestStatus::Fail);
        let SessionEvent::Complete(report) = validator.consume(end(RunResult::Fail)).unwrap()
        else {
            panic!("expected complete report");
        };
        assert!(!report.passed());
    }

    #[test]
    fn required_skip_produces_failing_report() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        feed_all_tests(&mut validator, TestStatus::Skip);
        let SessionEvent::Complete(report) = validator.consume(end(RunResult::Fail)).unwrap()
        else {
            panic!("expected complete report");
        };
        assert!(!report.passed());
    }

    #[test]
    fn required_block_produces_failing_report() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        feed_all_tests(&mut validator, TestStatus::Blocked);
        let SessionEvent::Complete(report) = validator.consume(end(RunResult::Fail)).unwrap()
        else {
            panic!("expected complete report");
        };
        assert!(!report.passed());
    }

    #[test]
    fn rejects_duplicate_test_id() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        let record = test("mdio.read-repeat", TestStatus::Pass);
        validator.consume(record.clone()).unwrap();
        assert_eq!(
            validator.consume(record),
            Err(SessionError::DuplicateTest("mdio.read-repeat".to_owned()))
        );
    }

    #[test]
    fn rejects_stale_run_id() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        assert_eq!(
            validator.consume(Record::Observation {
                run_id: 0xbeef,
                name: "x".to_owned(),
                value: "1".to_owned(),
            }),
            Err(SessionError::StaleRun {
                expected: RUN,
                actual: 0xbeef,
            })
        );
    }

    #[test]
    fn rejects_run_id_from_a_different_build() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true).expect_run_id(RUN + 1);
        assert_eq!(
            validator.consume(start(RUN, COMMIT)),
            Err(SessionError::StaleRun {
                expected: RUN + 1,
                actual: RUN,
            })
        );
    }

    #[test]
    fn rejects_stale_commit() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        assert_eq!(
            validator.consume(start(RUN, "deadbee")),
            Err(SessionError::StaleCommit {
                expected: COMMIT.to_owned(),
                actual: "deadbee".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_unexpected_reset() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        assert_eq!(
            validator.consume(start(RUN + 1, COMMIT)),
            Err(SessionError::UnexpectedReset)
        );
    }

    #[test]
    fn detects_missing_terminal() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        assert_eq!(validator.finish(), Err(SessionError::MissingTerminal));
    }

    #[test]
    fn detects_missing_test_ids() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        assert!(matches!(
            validator.consume(Record::RunEnd {
                run_id: RUN,
                result: RunResult::Pass,
            }),
            Err(SessionError::MissingTests(_))
        ));
    }

    #[test]
    fn rejects_firmware_pass_after_failure() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        validator.consume(start(RUN, COMMIT)).unwrap();
        feed_all_tests(&mut validator, TestStatus::Fail);
        assert_eq!(
            validator.consume(end(RunResult::Pass)),
            Err(SessionError::ResultMismatch {
                expected: RunResult::Fail,
                actual: RunResult::Pass,
            })
        );
    }

    #[test]
    fn rejects_fallback_run_in_release_mode() {
        let mut validator = SessionValidator::new(Suite::Mdio, COMMIT, true);
        assert_eq!(
            validator.consume(start(0, COMMIT)),
            Err(SessionError::FallbackRunId)
        );
    }
}
