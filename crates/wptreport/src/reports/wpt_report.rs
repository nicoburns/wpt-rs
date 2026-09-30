//! The standard "wptreport" format produced by the official wptrunner as well
//! as other wpt test runners.
use crate::{HasRunInfo, ScorableReport, SubtestCounts, SubtestNameAndResult, TestResultIter};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// The `run_info` object of a report, stored verbatim (same keys, key order and values) so that
/// reading and re-writing a report doesn't alter it. Use [`StandardRunInfo`] to construct one.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
#[serde(transparent)]
pub struct WptRunInfo(pub IndexMap<String, Value>);

impl WptRunInfo {
    fn get_str(&self, key: &str) -> Option<&str> {
        self.0.get(key)?.as_str()
    }

    /// The browser engine tested (e.g. "servo")
    pub fn product(&self) -> Option<&str> {
        self.get_str("product")
    }

    /// The version of the browser engine tested
    pub fn browser_version(&self) -> Option<&str> {
        self.get_str("browser_version")
    }

    /// The revision of the WPT test suite that run
    pub fn revision(&self) -> Option<&str> {
        self.get_str("revision")
    }

    /// Sets `browser_version`, keeping its existing position in the key order
    pub fn set_browser_version(&mut self, browser_version: impl Into<String>) {
        self.0.insert(
            String::from("browser_version"),
            Value::String(browser_version.into()),
        );
    }
}

impl From<StandardRunInfo> for WptRunInfo {
    fn from(run_info: StandardRunInfo) -> Self {
        // Round-trip through a string rather than `serde_json::Value` so that keys keep the
        // struct's field order (`serde_json::Map` sorts keys unless `preserve_order` is enabled)
        let json = serde_json::to_string(&run_info).unwrap();
        serde_json::from_str(&json).unwrap()
    }
}

/// The standard set of `run_info` keys written by wptrunner
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct StandardRunInfo {
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
    /// Linux distro (if linux)
    pub linux_distro: Option<String>,
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
    fn results(&self) -> impl Iterator<Item = Self::TestResultIter<'_>> {
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

    fn subtest_exist_and_passes(&self, name: &str) -> bool {
        self.subtests
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.status == SubtestStatus::Pass)
            .unwrap_or(false)
    }

    fn iter_subtests_results(&self) -> impl Iterator<Item = SubtestNameAndResult<'_>> {
        self.subtests
            .iter()
            .map(|s: &SubtestResult| SubtestNameAndResult {
                name: &s.name,
                passes: s.status == SubtestStatus::Pass,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real `run_info` objects from Servo's internal-wpt-dashboard runs (2023-04-01 and 2026-09-30)
    const RUN_INFO_2023: &str = r#"{"os":"linux","processor":"x86_64","version":"Ubuntu 20.04","os_version":"20.04","bits":64,"has_sandbox":true,"webrender":false,"automation":false,"linux_distro":"Ubuntu","revision":"901ba9bab01a70587411279d0cd47aab68bb2a85","python_version":3,"product":"servo","debug":false,"browser_version":"0.0.1-901ba9bab","verify":false,"wasm":false,"headless":false}"#;
    const RUN_INFO_2026: &str = r#"{"os":"linux","processor":"x86_64","version":"Ubuntu 22.04","os_version":"22.04","bits":64,"has_sandbox":true,"display":null,"automation":false,"linux_distro":"Ubuntu","apple_silicon":false,"apple_catalina":false,"win10_2004":false,"win10_2009":false,"win11_2009":false,"revision":"1cd1fadbec7e5ee8b931887b44a586fb136cfe62","python_version":3,"product":"servo","debug":false,"browser_version":"0.6.0-5698249bd","verify":false,"wasm":false,"headless":false}"#;

    #[test]
    fn round_trips_run_info_verbatim() {
        for json in [RUN_INFO_2023, RUN_INFO_2026] {
            let run_info: WptRunInfo = serde_json::from_str(json).unwrap();
            assert_eq!(serde_json::to_string(&run_info).unwrap(), json);
        }
    }

    #[test]
    fn reads_and_sets_known_keys() {
        let mut run_info: WptRunInfo = serde_json::from_str(RUN_INFO_2023).unwrap();
        assert_eq!(run_info.product(), Some("servo"));
        assert_eq!(run_info.browser_version(), Some("0.0.1-901ba9bab"));
        assert_eq!(
            run_info.revision(),
            Some("901ba9bab01a70587411279d0cd47aab68bb2a85")
        );

        run_info.set_browser_version("Unknown");
        let expected = RUN_INFO_2023.replace("0.0.1-901ba9bab", "Unknown");
        assert_eq!(serde_json::to_string(&run_info).unwrap(), expected);
    }

    #[test]
    fn converts_from_standard_run_info_in_field_order() {
        let standard = StandardRunInfo {
            product: String::from("blitz"),
            browser_version: Some(String::from("00255b1e7e65a6fbe8b37ea7656003ede1f45fe1")),
            revision: String::from("42a9dfcc6d5fda82e2bcd141c8cc8ee62e0166be"),
            automation: true,
            debug: false,
            display: None,
            has_sandbox: false,
            headless: true,
            verify: false,
            wasm: false,
            os: String::new(),
            os_version: String::new(),
            linux_distro: None,
            version: String::new(),
            processor: String::new(),
            bits: 64,
            python_version: 0,
            apple_catalina: false,
            apple_silicon: false,
            win10_2004: false,
            win10_2009: false,
            win11_2009: false,
        };
        let expected = serde_json::to_string(&standard).unwrap();
        let run_info = WptRunInfo::from(standard);
        assert_eq!(serde_json::to_string(&run_info).unwrap(), expected);
    }
}
