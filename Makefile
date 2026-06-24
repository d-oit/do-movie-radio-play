.PHONY: quality test lint fix fmt bench audit check-all

quality:
	bash scripts/quality_gate.sh

test:
	cargo test --workspace --all-features

lint:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

fix:
	bash scripts/quality_gate.sh --fix

fmt:
	cargo fmt --all

bench:
	cargo bench -p benchmarks --bench pipeline_bench -- --noplot

audit:
	cargo deny check

check-all:
	cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace --all-features

harness:
	bash scripts/harness-check.sh all

doctor:
	bash scripts/doctor.sh

skills:
	bash scripts/validate-skills.sh

agents:
	bash scripts/validate-agent-entrypoints.sh
