use crate::{
    wpt_report::{TestResult, WptReport},
    TestResultIter,
};
use std::collections::BTreeMap;

pub fn aggregate<T>(
    reports: &mut [WptReport],
    mut map_fn: impl FnMut(&[Option<&TestResult>]) -> T,
) -> Vec<T> {
    let report_count = reports.len();
    assert!(report_count <= 64);

    let mut test_names: BTreeMap<String, u64> = BTreeMap::new();
    for (i, report) in reports.iter_mut().enumerate() {
        report.results.sort_by(|a, b| a.test.cmp(&b.test));
        let mask = 1 << i;
        for result in &report.results {
            test_names
                .entry(result.test.clone())
                .and_modify(|bitset| *bitset |= mask)
                .or_insert(mask);
        }
    }

    let mut results = Vec::with_capacity(test_names.len());
    let mut iterators = reports
        .iter_mut()
        .map(|report| report.results.iter().peekable())
        .collect::<Vec<_>>();
    let mut current_row: Vec<Option<&TestResult>> = vec![None; report_count];

    for (_, bitvec) in test_names {
        for (i, item) in current_row.iter_mut().enumerate() {
            if (bitvec & (1 << i as u64)) != 0 {
                *item = iterators[i].next()
            } else {
                *item = None
            }
        }

        results.push(map_fn(&current_row));
    }

    results
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DiffStatus {
    Same,
    Added,
    Removed,
    Changed,
}

pub struct TestDiff {
    pub status: DiffStatus,
}

pub fn diff(reports: &mut [WptReport; 2]) -> Vec<TestDiff> {
    aggregate(&mut *reports, |results| {
        let a = results[0];
        let b = results[1];

        let status = match (a, b) {
            (None, None) => unreachable!(),
            (Some(_), None) => DiffStatus::Removed,
            (None, Some(_)) => DiffStatus::Added,
            (Some(a), Some(b)) => {
                if a.subtest_counts() == b.subtest_counts() {
                    DiffStatus::Same
                } else {
                    DiffStatus::Changed
                }
            }
        };

        TestDiff { status }
    })
}
