#!/usr/bin/env python3
import argparse
import json
import math
from pathlib import Path


def wilson_lower_bound(p: float, n: int, z: float = 1.96) -> float:
    if n <= 0:
        return 0.0
    p = max(0.0, min(1.0, p))
    z2 = z * z
    denom = 1.0 + z2 / n
    center = p + z2 / (2.0 * n)
    margin = z * math.sqrt((p * (1.0 - p) / n) + (z2 / (4.0 * n * n)))
    return max(0.0, (center - margin) / denom)


def metric_value(metrics: dict, key: str) -> float:
    aliases = {
        "non_voice_precision": ["non_voice_time_precision", "non_voice_precision"],
        "non_voice_recall": ["non_voice_time_recall", "non_voice_recall"],
    }
    for candidate in aliases.get(key, [key]):
        value = metrics.get(candidate)
        if value is not None:
            return float(value)
    value = metrics.get(key)
    if value is None:
        return 0.0
    return float(value)


def sample_size_from_ms(metrics: dict, key: str, fallback: int) -> int:
    raw = metrics.get(key)
    if raw is None:
        return max(fallback, 1)
    ms = int(raw)
    if ms <= 0:
        return max(fallback, 1)
    return max(ms // 100, 1)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Gate radio-play holdout readiness with Wilson LB95 confidence bounds"
    )
    parser.add_argument(
        "--summary",
        default="analysis/validation/full-sweep-summary.json",
        help="Validation sweep summary JSON",
    )
    parser.add_argument(
        "--holdout-tier",
        default="C",
        help="Tier used as holdout",
    )
    parser.add_argument(
        "--min-lb95",
        type=float,
        default=0.95,
        help="Minimum required Wilson lower bound (95%% confidence)",
    )
    args = parser.parse_args()

    summary_path = Path(args.summary)
    if not summary_path.exists():
        raise FileNotFoundError(f"missing summary file: {summary_path}")

    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    holdout_tier = args.holdout_tier.upper()
    results = summary.get("results", [])
    holdout = [r for r in results if str(r.get("tier", "")).upper() == holdout_tier]
    if not holdout:
        raise ValueError(f"no holdout entries for tier={holdout_tier}")

    checks = []
    failures = []
    for item in holdout:
        metrics = item.get("metrics", {})
        expected_segments = int(metrics.get("expected_segments") or 0)
        predicted_segments = int(metrics.get("predicted_segments") or 0)
        expected_n = sample_size_from_ms(
            metrics, "non_voice_expected_ms", expected_segments
        )
        predicted_n = sample_size_from_ms(
            metrics, "non_voice_predicted_ms", predicted_segments
        )

        precision = metric_value(metrics, "non_voice_precision")
        recall = metric_value(metrics, "non_voice_recall")
        overlap = metric_value(metrics, "overlap_ratio")

        precision_lb95 = wilson_lower_bound(precision, predicted_n)
        recall_lb95 = wilson_lower_bound(recall, expected_n)
        overlap_lb95 = wilson_lower_bound(overlap, expected_n)

        entry_id = item.get("id", "unknown")
        entry = {
            "id": entry_id,
            "expected_segments": expected_segments,
            "predicted_segments": predicted_segments,
            "expected_non_voice_samples": expected_n,
            "predicted_non_voice_samples": predicted_n,
            "non_voice_precision": precision,
            "non_voice_recall": recall,
            "overlap_ratio": overlap,
            "non_voice_precision_lb95": precision_lb95,
            "non_voice_recall_lb95": recall_lb95,
            "overlap_ratio_lb95": overlap_lb95,
        }
        checks.append(entry)

        if precision_lb95 < args.min_lb95:
            failures.append(
                f"{entry_id}: precision_lb95={precision_lb95:.4f} < {args.min_lb95:.4f}"
            )
        if recall_lb95 < args.min_lb95:
            failures.append(
                f"{entry_id}: recall_lb95={recall_lb95:.4f} < {args.min_lb95:.4f}"
            )
        if overlap_lb95 < args.min_lb95:
            failures.append(
                f"{entry_id}: overlap_lb95={overlap_lb95:.4f} < {args.min_lb95:.4f}"
            )

    report = {
        "summary": str(summary_path),
        "holdout_tier": holdout_tier,
        "min_lb95": args.min_lb95,
        "passed": len(failures) == 0,
        "checks": checks,
        "failures": failures,
    }
    print(json.dumps(report, indent=2))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
