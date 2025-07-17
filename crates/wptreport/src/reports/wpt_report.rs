//! The standard "wptreport" format produced by the official wptrunner as well
//! as other wpt test runners.
use crate::{HasRunInfo, ScorableReport, SubtestCounts, TestResultIter};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TestStatus {
    Pass,
    Fail,
    Ok,
    Error,
    Timeout,
    Crash,
    Assert,
    PreconditionFailed,
    Skip,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubtestStatus {
    Pass,
    Fail,
    Error,
    Timeout,
    Assert,
    PreconditionFailed,
    Notrun,
    Skip,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WptReport {
    pub time_start: u64,
    pub time_end: u64,
    pub run_info: WptRunInfo,
    pub results: Vec<TestResult>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct WptRunInfo {
    /// The browser engine tested (e.g. "servo")
    pub product: String,
    /// The version of the browser engine tested
    pub browser_version: Option<String>,
    /// The revision of the WPT test suite that run
    pub revision: String,

    // Flags
    pub automation: bool,
    pub debug: bool,
    pub display: Option<String>,
    pub has_sandbox: bool,
    pub headless: bool,
    pub verify: bool,
    pub wasm: bool,

    /// The OS that the tests were run on (e.g. "macos")
    pub os: String,
    /// OS version number
    pub os_version: String,
    /// OS version String
    pub version: String,
    /// The processor architecture the tests were run on (e.g. "arm")
    pub processor: String,
    /// The number of bits that the processor has (e.g. 64 for x86_64)
    pub bits: i64,
    /// The Python version used to run the tests
    pub python_version: i64,

    // OS Flags
    #[serde(default)]
    pub apple_catalina: bool,
    #[serde(default)]
    pub apple_silicon: bool,
    #[serde(default)]
    pub win10_2004: bool,
    #[serde(default)]
    pub win10_2009: bool,
    #[serde(default)]
    pub win11_2009: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestResult {
    pub test: String,
    pub status: TestStatus,
    pub duration: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub message: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub known_intermittent: Vec<String>,

    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub subsuite: String,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub subtests: Vec<SubtestResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubtestResult {
    pub name: String,
    pub status: SubtestStatus,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub message: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub known_intermittent: Vec<String>,
}

#[rustfmt::skip]
impl ScorableReport for WptReport {
    type TestResultIter<'a> = &'a TestResult where Self: 'a;
    type TestIter<'a> = core::slice::Iter<'a, TestResult> where Self: 'a;

    fn results(&self) -> Self::TestIter<'_> {
        self.results.iter()
    }
}

impl HasRunInfo for WptReport {
    fn run_info(&self) -> &WptRunInfo {
        &self.run_info
    }
}

impl TestResultIter for &TestResult {
    fn name(&self) -> &str {
        &self.test
    }

    fn subtest_counts(&self) -> SubtestCounts {
        let total = self.subtests.len() as u32;

        if total == 0 {
            SubtestCounts {
                total: 1,
                pass: (self.status == TestStatus::Pass) as u32,
            }
        } else {
            let pass = self.subtests.iter().fold(0, |mut pass_count, subtest| {
                pass_count += (subtest.status == SubtestStatus::Pass) as u32;
                pass_count
            });
            SubtestCounts { pass, total }
        }
    }
}
