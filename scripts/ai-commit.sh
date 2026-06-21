#!/usr/bin/env bash
set -euo pipefail
bash scripts/quality_gate.sh && git add -A && git commit "$@"
