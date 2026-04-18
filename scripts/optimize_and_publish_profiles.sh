#!/usr/bin/env bash
set -euo pipefail

report_path=${1:-analysis/optimization/fp-sweep-ranked-latest.json}
max_candidates=${2:-20}
min_coverage_ratio=${3:-0.7}

previous_report="analysis/optimization/fp-sweep-ranked.json"
comparison_report="analysis/optimization/fp-sweep-comparison.json"
changelog_note="analysis/learnings/latest-optimization-note.md"

echo "[1/4] Running optimization sweep..."
python3 scripts/optimize_fp_sweep.py \
  --expand-candidates \
  --max-candidates "$max_candidates" \
  --min-coverage-ratio "$min_coverage_ratio" \
  --output "$report_path"

if [[ -f "$previous_report" ]]; then
  echo "[2/4] Comparing against previous sweep..."
  python3 scripts/compare_sweeps.py \
    --previous "$previous_report" \
    --current "$report_path" \
    --output "$comparison_report"
else
  echo "[2/4] No previous sweep found; skipping comparison"
fi

echo "[3/4] Generating optimized profiles..."
python3 scripts/generate_optimized_profiles.py \
  --sweep-report "$report_path" \
  --modern-output config/profiles/modern-optimized.json \
  --legacy-output config/profiles/legacy-optimized.json

echo "[4/4] Writing compact changelog note..."
SWEEP_REPORT_PATH="$report_path" python3 - <<'PY'
import json
import os
from pathlib import Path

report_path = Path(os.environ["SWEEP_REPORT_PATH"])

comparison_path = Path("analysis/optimization/fp-sweep-comparison.json")
note_path = Path("analysis/learnings/latest-optimization-note.md")

report = json.loads(report_path.read_text(encoding="utf-8"))
best = report.get("best_candidate", {})
candidate = best.get("candidate", {}).get("name")
fp = best.get("weighted_false_positive_rate")
risk = best.get("weighted_false_positive_risk_rate")

lines = [
    "# Latest Optimization Note",
    "",
    f"- Sweep report: `{report_path}`",
    f"- Winner: `{candidate}`",
    f"- weighted_false_positive_rate: `{fp}`",
    f"- weighted_false_positive_risk_rate: `{risk}`",
]

if comparison_path.exists():
    comp = json.loads(comparison_path.read_text(encoding="utf-8"))
    lines.extend(
        [
            "",
            "## Comparison vs previous",
            f"- previous winner: `{comp.get('previous_winner')}`",
            f"- winner changed: `{comp.get('winner_changed')}`",
            f"- weighted FP delta: `{comp.get('weighted_false_positive_rate_delta')}`",
            f"- weighted risk delta: `{comp.get('weighted_false_positive_risk_rate_delta')}`",
        ]
    )

note_path.parent.mkdir(parents=True, exist_ok=True)
note_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"wrote compact note: {note_path}")
PY

# Keep canonical report path synced for downstream tooling.
cp "$report_path" "$previous_report"

echo "done: sweep + compare + profiles + compact note"
