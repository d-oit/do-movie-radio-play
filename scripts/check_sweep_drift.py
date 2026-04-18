#!/usr/bin/env python3
import argparse
import json
import sys
from pathlib import Path


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Fail when sweep comparison indicates unacceptable drift"
    )
    parser.add_argument(
        "--comparison",
        required=True,
        help="Path to fp-sweep-comparison.json",
    )
    parser.add_argument(
        "--max-fp-delta",
        type=float,
        default=0.02,
        help="Maximum allowed weighted false positive rate increase",
    )
    parser.add_argument(
        "--max-risk-delta",
        type=float,
        default=0.02,
        help="Maximum allowed weighted false positive risk increase",
    )
    parser.add_argument(
        "--allow-winner-change",
        action="store_true",
        help="Allow winner changes without failing when deltas are within thresholds",
    )
    args = parser.parse_args()

    comparison_path = Path(args.comparison)
    if not comparison_path.exists():
        print(f"comparison file missing: {comparison_path}", file=sys.stderr)
        return 2

    data = load_json(comparison_path)
    winner_changed = bool(data.get("winner_changed", False))
    fp_delta = float(data.get("weighted_false_positive_rate_delta", 0.0) or 0.0)
    risk_delta = float(data.get("weighted_false_positive_risk_rate_delta", 0.0) or 0.0)

    failures: list[str] = []
    if fp_delta > args.max_fp_delta:
        failures.append(
            f"weighted FP delta {fp_delta:.6f} exceeds max {args.max_fp_delta:.6f}"
        )
    if risk_delta > args.max_risk_delta:
        failures.append(
            f"weighted risk delta {risk_delta:.6f} exceeds max {args.max_risk_delta:.6f}"
        )
    if winner_changed and not args.allow_winner_change:
        failures.append("winner changed unexpectedly")

    if failures:
        print("sweep drift check failed:", file=sys.stderr)
        for line in failures:
            print(f"- {line}", file=sys.stderr)
        return 1

    print("sweep drift check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
