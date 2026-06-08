#!/bin/bash
set -euo pipefail
echo "Running quality checks..."
exec bash scripts/quality_gate.sh
