# WPT-RS

[![Crates.io](https://img.shields.io/crates/v/wpt_report.svg)](https://crates.io/crates/wpt_report)
[![Docs](https://docs.rs/wpt_report/badge.svg)](https://docs.rs/wpt_report)
![Crates.io License](https://img.shields.io/crates/l/wpt_report)

Tools for working with Web Platform Tests (WPT tests) in Rust.

Currently there is:

- The `wpt_report` library which allows to parse, generate and manipulate reports in the "WPT report" JSON format.
- The `wpt` CLI which exposes many of the functions in `wpt_report` as a bianry tool

## Installing the `wpt` CLI

Prebuilt binaries are attached to each [GitHub release](https://github.com/nicoburns/wpt-rs/releases) and can be installed with [cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall wpt
```

Or build from source with `cargo install wpt`.

## Releasing

Publish to crates.io, then push a `v<version>` tag matching the workspace version (e.g. `v0.1.2`). The release workflow creates the GitHub release and uploads `wpt` binaries for Linux (x86_64, aarch64), macOS (x86_64, aarch64) and Windows (x86_64, aarch64).
