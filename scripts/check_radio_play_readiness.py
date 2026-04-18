#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def load_summary(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def metric_value(metrics: dict, key: str) -> float:
    value = metrics.get(key)
    if value is None:
        return 0.0
    return float(value)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check holdout radio-play readiness from validation sweep summary"
    )
    parser.add_argument(
        "--summary",
        default="analysis/validation/full-sweep-summary.json",
        help="Validation sweep summary JSON",
    )
    parser.add_argument(
        "--holdout-tier",
        default="C",
        help="Tier to treat as holdout for readiness gate",
    )
    parser.add_argument(
        "--min-non-voice-precision",
        type=float,
        default=0.95,
        help="Minimum required non_voice_precision on holdout entries",
    )
    parser.add_argument(
        "--min-non-voice-recall",
        type=float,
        default=0.95,
        help="Minimum required non_voice_recall on holdout entries",
    )
    parser.add_argument(
        "--min-overlap",
        type=float,
        default=0.95,
        help="Minimum required overlap_ratio on holdout entries",
    )
    args = parser.parse_args()

    summary_path = Path(args.summary)
    if not summary_path.exists():
        raise FileNotFoundError(f"missing summary file: {summary_path}")

    summary = load_summary(summary_path)
    holdout_tier = args.holdout_tier.upper()
    results = summary.get("results", [])
    holdout_results = [
        r for r in results if str(r.get("tier", "")).upper() == holdout_tier
    ]
    if not holdout_results:
        raise ValueError(f"no holdout entries found for tier={holdout_tier}")

    failures = []
    checks = []
    for result in holdout_results:
        metrics = result.get("metrics", {})
        precision = metric_value(metrics, "non_voice_precision")
        recall = metric_value(metrics, "non_voice_recall")
        overlap = metric_value(metrics, "overlap_ratio")
        entry_id = result.get("id", "unknown")

        checks.append(
            {
                "id": entry_id,
                "non_voice_precision": precision,
                "non_voice_recall": recall,
                "overlap_ratio": overlap,
            }
        )

        if precision < args.min_non_voice_precision:
            failures.append(
                f"{entry_id}: non_voice_precision={precision:.4f} < {args.min_non_voice_precision:.4f}"
            )
        if recall < args.min_non_voice_recall:
            failures.append(
                f"{entry_id}: non_voice_recall={recall:.4f} < {args.min_non_voice_recall:.4f}"
            )
        if overlap < args.min_overlap:
            failures.append(
                f"{entry_id}: overlap_ratio={overlap:.4f} < {args.min_overlap:.4f}"
            )

    report = {
        "summary": str(summary_path),
        "holdout_tier": holdout_tier,
        "thresholds": {
            "min_non_voice_precision": args.min_non_voice_precision,
            "min_non_voice_recall": args.min_non_voice_recall,
            "min_overlap": args.min_overlap,
        },
        "checks": checks,
        "passed": len(failures) == 0,
        "failures": failures,
    }

    print(json.dumps(report, indent=2))
    if failures:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
