check:
  cargo check --workspace

clippy:
  cargo clippy --workspace

fmt:
  cargo fmt --all

wpt *ARGS:
  cargo run -rp wpt -- {{ARGS}}

recalc:
  cargo run -rp wpt -- calc-scores --in ../internal-wpt-dashboard/runs-2020 --out scores.json --focus-areas ../internal-wpt-dashboard/focus-areas.json