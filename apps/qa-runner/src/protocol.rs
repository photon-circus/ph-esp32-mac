//! Allocation-free PHQA serial protocol.

use core::fmt::Display;

/// PHQA wire protocol version.
pub const PROTOCOL_VERSION: u8 = 1;

/// Whether a suite is exploratory or release-gating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    /// Human-driven diagnostics that may continue monitoring after completion.
    Exploratory,
    /// Deterministic release validation that terminates after its final record.
    Release,
}

impl RunMode {
    const fn token(self) -> &'static str {
        match self {
            Self::Exploratory => "exploratory",
            Self::Release => "release",
        }
    }
}

/// Outcome of one QA test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestResult {
    /// The required observation was made.
    Pass,
    /// The test ran and its assertion failed.
    Fail,
    /// The test could not run because a runtime prerequisite was absent.
    Skip,
    /// The test cannot run because required harness support is not implemented.
    Blocked,
}

impl TestResult {
    /// Human-readable symbol used by the exploratory log.
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Pass => "✓",
            Self::Fail => "✗",
            Self::Skip => "○",
            Self::Blocked => "!",
        }
    }

    const fn token(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Skip => "SKIP",
            Self::Blocked => "BLOCKED",
        }
    }
}

/// Accumulated suite statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TestStats {
    /// Number of passing tests.
    pub passed: u32,
    /// Number of failing tests.
    pub failed: u32,
    /// Number of skipped tests.
    pub skipped: u32,
    /// Number of blocked tests.
    pub blocked: u32,
    /// Number of required tests that were skipped or blocked.
    pub required_incomplete: u32,
}

impl TestStats {
    /// Create empty statistics.
    pub const fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
            skipped: 0,
            blocked: 0,
            required_incomplete: 0,
        }
    }

    fn record(&mut self, result: TestResult, required: bool) {
        match result {
            TestResult::Pass => self.passed += 1,
            TestResult::Fail => self.failed += 1,
            TestResult::Skip => self.skipped += 1,
            TestResult::Blocked => self.blocked += 1,
        }

        if required && matches!(result, TestResult::Skip | TestResult::Blocked) {
            self.required_incomplete += 1;
        }
    }

    /// Number of emitted test records.
    pub const fn total(self) -> u32 {
        self.passed + self.failed + self.skipped + self.blocked
    }

    /// Whether every required test passed.
    pub const fn all_passed(self) -> bool {
        self.failed == 0 && self.required_incomplete == 0
    }
}

/// Terminal suite outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunResult {
    /// Every required test passed.
    Pass,
    /// At least one test failed or a required test was incomplete.
    Fail,
}

impl RunResult {
    const fn token(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
        }
    }
}

/// Stateful emitter for exact PHQA protocol records.
///
/// All fields are borrowed string tokens and every record includes the run
/// identifier so the host can reject stale serial output.
pub struct Reporter<'a> {
    run_id: &'a str,
    suite: &'a str,
    commit: &'a str,
    mode: RunMode,
    reset_reason: &'a str,
    stats: TestStats,
    ready_step: u32,
}

impl<'a> Reporter<'a> {
    /// Create a reporter for one firmware run.
    pub const fn new(
        run_id: &'a str,
        suite: &'a str,
        commit: &'a str,
        mode: RunMode,
        reset_reason: &'a str,
    ) -> Self {
        Self {
            run_id,
            suite,
            commit,
            mode,
            reset_reason,
            stats: TestStats::new(),
            ready_step: 0,
        }
    }

    /// Emit the opening run record.
    pub fn start(&self) {
        esp_println::println!(
            "PHQA|{}|RUN_START|run={}|suite={}|commit={}|mode={}|reset_reason={}",
            PROTOCOL_VERSION,
            self.run_id,
            self.suite,
            self.commit,
            self.mode.token(),
            self.reset_reason
        );
    }

    /// Request a synchronized action from the host controller.
    pub fn ready(&mut self, action: &str) {
        self.ready_step = self.ready_step.wrapping_add(1);
        esp_println::println!(
            "PHQA|{}|READY|run={}|step={}|action={}",
            PROTOCOL_VERSION,
            self.run_id,
            self.ready_step,
            action
        );
    }

    /// Emit one named observation.
    pub fn observation(&self, name: &str, value: impl Display) {
        esp_println::println!(
            "PHQA|{}|OBS|run={}|name={}|value={}",
            PROTOCOL_VERSION,
            self.run_id,
            name,
            value
        );
    }

    /// Record and emit one test result.
    ///
    /// A required `SKIP` or `BLOCKED` result makes the complete run fail.
    pub fn record(&mut self, id: &str, result: TestResult, required: bool) {
        self.record_with_reason(id, result, required, "unspecified");
    }

    /// Record and emit one test result with a machine-readable reason token.
    ///
    /// Reasons must be single ASCII tokens without `|` delimiters.
    pub fn record_with_reason(
        &mut self,
        id: &str,
        result: TestResult,
        required: bool,
        reason: &str,
    ) {
        self.stats.record(result, required);
        if result == TestResult::Pass {
            esp_println::println!(
                "PHQA|{}|TEST|run={}|id={}|status={}|required={}",
                PROTOCOL_VERSION,
                self.run_id,
                id,
                result.token(),
                u8::from(required)
            );
        } else {
            esp_println::println!(
                "PHQA|{}|TEST|run={}|id={}|status={}|required={}|reason={}",
                PROTOCOL_VERSION,
                self.run_id,
                id,
                result.token(),
                u8::from(required),
                reason
            );
        }
    }

    /// Return the current counters.
    pub const fn stats(&self) -> TestStats {
        self.stats
    }

    /// Return the number of test records emitted so far.
    pub const fn total(&self) -> u32 {
        self.stats.total()
    }

    /// Return whether every required result recorded so far passed.
    pub const fn all_passed(&self) -> bool {
        self.stats.all_passed()
    }

    /// Emit the terminal run record and return its outcome.
    pub fn finish(&self) -> RunResult {
        let result = if self.stats.all_passed() {
            RunResult::Pass
        } else {
            RunResult::Fail
        };

        esp_println::println!(
            "PHQA|{}|RUN_END|run={}|result={}",
            PROTOCOL_VERSION,
            self.run_id,
            result.token()
        );
        result
    }
}
