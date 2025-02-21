mod report;
mod score;

pub use report::{
    RunInfo, SubtestResult, SubtestStatus, TestResult, TestScore, TestStatus, WptReport, WptScores,
};
pub use score::{score_wpt_report, TestResultIter, AreaScores};

// Use jemalloc as the allocator
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
