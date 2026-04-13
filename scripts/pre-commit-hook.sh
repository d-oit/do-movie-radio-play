#!/bin/bash
set -e
echo "Running quality checks..."
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
echo "Pre-commit checks passed!"
