#!/usr/bin/env bash
set -euo pipefail

# scripts/update-all-docs.sh — Regenerates all documentation reports

echo "==> Building radio play readiness report"
python3 scripts/build_radio_play_readiness_report.py

echo "==> Building radio play failure breakdown"
python3 scripts/build_radio_play_failure_breakdown.py

echo "✓ All reports updated"
