#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check
cargo clippy --all-targets --all-features
cargo test --all-targets --all-features
cargo build --release
