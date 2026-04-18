#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def candidate_name(report: dict) -> str | None:
    best = report.get("best_candidate") or {}
    return best.get("candidate", {}).get("name")


def metric(report: dict, key: str) -> float | None:
    best = report.get("best_candidate") or {}
    value = best.get(key)
    if value is None:
        return None
    return float(value)


def main() -> int:
    parser = argparse.ArgumentParser(description="Compare two FP sweep reports")
    parser.add_argument("--previous", required=True, help="Previous report path")
    parser.add_argument("--current", required=True, help="Current report path")
    parser.add_argument("--output", required=True, help="Output comparison JSON")
    args = parser.parse_args()

    previous_path = Path(args.previous)
    current_path = Path(args.current)
    output_path = Path(args.output)

    previous = load_json(previous_path)
    current = load_json(current_path)

    previous_name = candidate_name(previous)
    current_name = candidate_name(current)
    previous_fp = metric(previous, "weighted_false_positive_rate")
    current_fp = metric(current, "weighted_false_positive_rate")
    previous_risk = metric(previous, "weighted_false_positive_risk_rate")
    current_risk = metric(current, "weighted_false_positive_risk_rate")

    fp_delta = None
    if previous_fp is not None and current_fp is not None:
        fp_delta = current_fp - previous_fp

    risk_delta = None
    if previous_risk is not None and current_risk is not None:
        risk_delta = current_risk - previous_risk

    comparison = {
        "previous_report": str(previous_path),
        "current_report": str(current_path),
        "winner_changed": previous_name != current_name,
        "previous_winner": previous_name,
        "current_winner": current_name,
        "previous_weighted_false_positive_rate": previous_fp,
        "current_weighted_false_positive_rate": current_fp,
        "weighted_false_positive_rate_delta": fp_delta,
        "previous_weighted_false_positive_risk_rate": previous_risk,
        "current_weighted_false_positive_risk_rate": current_risk,
        "weighted_false_positive_risk_rate_delta": risk_delta,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(comparison, indent=2) + "\n", encoding="utf-8")
    print(f"wrote sweep comparison: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
