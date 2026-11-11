# wpt

[![Crates.io](https://img.shields.io/crates/v/wpt.svg)](https://crates.io/crates/wpt)

A CLI for working with Web Platform Tests (WPT) data: diffing, merging, converting and scoring WPT reports, and generating `MANIFEST.json` files.

## Installing

Prebuilt binaries are attached to each [GitHub release](https://github.com/nicoburns/wpt-rs/releases) and can be installed with [cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall wpt
```

Or build from source with `cargo install wpt`.

## Usage

Run `wpt --help` (or `wpt <command> --help`) for the full list of options. Report inputs are WPT report JSON files (as written by `./wpt run --log-wptreport`), and any input file ending in `.xz` or `.zst` is decompressed on the fly.

### `wpt diff`

Compare two WPT reports and list every test whose status or passing subtest count changed:

```sh
wpt diff before.json after.json
```

```
OK => OK  [2/2]  +1  /css/a.html
REM       [0/1]  +0  /css/b.html
ADD       [1/1]  +1  /css/c.html
====================
Subtests: 2 newly passing, 0 newly failing (net +2)
Tests:    1 added, 1 removed
Crashes:  0 (+0)
Timeouts: 0 (+0)
Done in 0ms
```

Each row shows the test status before and after (or `ADD`/`REM`), the passing/total subtests in the new report, and the change in passing subtests.

- `-v`, `--verbose`: also list the individual subtests that changed under each test.
- `--format json`: print the diff as JSON instead of text.

### `wpt merge`

Merge the chunks of a chunked test run into a single report. Every file in the `--in` directory (except hidden files) is read as one chunk, and all chunks must have the same `run_info`:

```sh
wpt merge --in chunks/ --out report.json
```

### `wpt convert`

Convert a WPT report into Servo's test scores format (the per-test and per-subtest scores used by the Servo WPT dashboard):

```sh
wpt convert --in report.json --out scores.json
```

- `--js-sort`: sort tests in JavaScript object iteration order.

### `wpt calc-scores`

Calculate scores, either for a single WPT report or for a directory of runs:

```sh
# Print the pass rate of each top-level area and write the per-area scores as JSON
wpt calc-scores --in report.json --out scores-by-area.json

# Build a score summary over time from a directory of Servo scores reports
wpt calc-scores --in runs/ --out scores.json --focus-areas focus-areas.json
```

In directory mode, each file name must start with the run's date (`YYYY-MM-DD`), every run is scored against the tests in the most recent run, and `--focus-areas` optionally gives the JSON list of areas to summarise.

### `wpt manifest build`

Build a `MANIFEST.json` for a web-platform-tests checkout, identical to `./wpt manifest --rebuild`:

```sh
wpt manifest build --tests-root path/to/wpt
```

- `--tests-root`: root of the WPT checkout (default: the current directory).
- `-p`, `--path`: where to write the manifest (default: `<TESTS_ROOT>/MANIFEST.json`).
- `--url-base`: base URL of the tests (default: `/`).
