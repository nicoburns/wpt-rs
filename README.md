# WPT-RS

[![Crates.io](https://img.shields.io/crates/v/wpt_report.svg)](https://crates.io/crates/wpt_report)
[![Docs](https://docs.rs/wpt_report/badge.svg)](https://docs.rs/wpt_report)
![Crates.io License](https://img.shields.io/crates/l/wpt_report)

Tools for working with Web Platform Tests (WPT tests) in Rust.

Currently there is:

- The `wpt_report` library which allows to parse, generate and manipulate reports in the "WPT report" JSON format.
- The `wpt_manifest` library, a port of WPT's `MANIFEST.json` generator (BSD-3-Clause, like upstream WPT). `wpt manifest build --tests-root <wpt checkout>` writes a manifest identical to `./wpt manifest --rebuild`.
- The `wpt` CLI which exposes many of the functions in `wpt_report` as a bianry tool
