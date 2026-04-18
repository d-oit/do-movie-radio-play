#!/usr/bin/env python3
import argparse
import json
import math
from pathlib import Path


def metric_value(metrics: dict, key: str) -> float:
    value = metrics.get(key)
    if value is None:
        return 0.0
    return float(value)


def wilson_lower_bound(p: float, n: int, z: float = 1.96) -> float:
    if n <= 0:
        return 0.0
    p = max(0.0, min(1.0, p))
    z2 = z * z
    denom = 1.0 + z2 / n
    center = p + z2 / (2.0 * n)
    margin = z * math.sqrt((p * (1.0 - p) / n) + (z2 / (4.0 * n * n)))
    return max(0.0, (center - margin) / denom)


def cohort_for_media(media_path: str) -> str:
    normalized = media_path.lower()
    legacy_markers = ["the_hole_1962", "windy_day_1967"]
    return (
        "legacy" if any(marker in normalized for marker in legacy_markers) else "modern"
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build a consolidated radio-play readiness report"
    )
    parser.add_argument(
        "--summary",
        default="analysis/validation/full-sweep-summary.json",
        help="Validation summary JSON",
    )
    parser.add_argument("--holdout-tier", default="C", help="Holdout tier")
    parser.add_argument(
        "--min-non-voice-precision", type=float, default=0.95, help="Min precision"
    )
    parser.add_argument(
        "--min-non-voice-recall", type=float, default=0.95, help="Min recall"
    )
    parser.add_argument("--min-overlap", type=float, default=0.95, help="Min overlap")
    parser.add_argument("--min-lb95", type=float, default=0.95, help="Min LB95")
    parser.add_argument(
        "--output-json",
        default="analysis/validation/radio-play-readiness-report.json",
        help="Output JSON path",
    )
    parser.add_argument(
        "--output-md",
        default="analysis/learnings/latest-radio-play-readiness-report.md",
        help="Output markdown path",
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
        raise ValueError(f"no holdout entries found for tier={holdout_tier}")

    threshold_failures = []
    lb95_failures = []
    failure_rows = []
    cohort_rows = {"modern": [], "legacy": []}

    for item in holdout:
        metrics = item.get("metrics", {})
        entry_id = item.get("id", "unknown")
        media = str(item.get("input_media") or entry_id)

        precision = metric_value(metrics, "non_voice_precision")
        recall = metric_value(metrics, "non_voice_recall")
        overlap = metric_value(metrics, "overlap_ratio")
        expected_segments = int(metrics.get("expected_segments") or 0)
        predicted_segments = int(metrics.get("predicted_segments") or 0)

        precision_lb95 = wilson_lower_bound(precision, max(predicted_segments, 1))
        recall_lb95 = wilson_lower_bound(recall, max(expected_segments, 1))
        overlap_lb95 = wilson_lower_bound(overlap, max(expected_segments, 1))

        row = {
            "id": entry_id,
            "media": media,
            "tier": item.get("tier", "unknown"),
            "cohort": cohort_for_media(media),
            "non_voice_precision": precision,
            "non_voice_recall": recall,
            "overlap_ratio": overlap,
            "non_voice_precision_lb95": precision_lb95,
            "non_voice_recall_lb95": recall_lb95,
            "overlap_ratio_lb95": overlap_lb95,
            "expected_segments": expected_segments,
            "predicted_segments": predicted_segments,
        }

        if precision < args.min_non_voice_precision:
            threshold_failures.append(
                f"{entry_id}: non_voice_precision={precision:.4f} < {args.min_non_voice_precision:.4f}"
            )
        if recall < args.min_non_voice_recall:
            threshold_failures.append(
                f"{entry_id}: non_voice_recall={recall:.4f} < {args.min_non_voice_recall:.4f}"
            )
        if overlap < args.min_overlap:
            threshold_failures.append(
                f"{entry_id}: overlap_ratio={overlap:.4f} < {args.min_overlap:.4f}"
            )

        if precision_lb95 < args.min_lb95:
            lb95_failures.append(
                f"{entry_id}: precision_lb95={precision_lb95:.4f} < {args.min_lb95:.4f}"
            )
        if recall_lb95 < args.min_lb95:
            lb95_failures.append(
                f"{entry_id}: recall_lb95={recall_lb95:.4f} < {args.min_lb95:.4f}"
            )
        if overlap_lb95 < args.min_lb95:
            lb95_failures.append(
                f"{entry_id}: overlap_lb95={overlap_lb95:.4f} < {args.min_lb95:.4f}"
            )

        if (
            precision < args.min_non_voice_precision
            or recall < args.min_non_voice_recall
            or overlap < args.min_overlap
            or precision_lb95 < args.min_lb95
            or recall_lb95 < args.min_lb95
            or overlap_lb95 < args.min_lb95
        ):
            failure_rows.append(row)

        cohort_rows[row["cohort"]].append(row)

    def avg(rows: list[dict], key: str) -> float:
        if not rows:
            return 0.0
        return sum(float(r[key]) for r in rows) / len(rows)

    cohorts = {
        name: {
            "count": len(rows),
            "avg_overlap_ratio": avg(rows, "overlap_ratio"),
            "avg_non_voice_precision": avg(rows, "non_voice_precision"),
            "avg_non_voice_recall": avg(rows, "non_voice_recall"),
            "avg_overlap_ratio_lb95": avg(rows, "overlap_ratio_lb95"),
            "avg_non_voice_precision_lb95": avg(rows, "non_voice_precision_lb95"),
            "avg_non_voice_recall_lb95": avg(rows, "non_voice_recall_lb95"),
        }
        for name, rows in cohort_rows.items()
    }

    threshold_gate_pass = len(threshold_failures) == 0
    lb95_gate_pass = len(lb95_failures) == 0
    readiness_pass = threshold_gate_pass and lb95_gate_pass

    report = {
        "summary": str(summary_path),
        "holdout_tier": holdout_tier,
        "thresholds": {
            "min_non_voice_precision": args.min_non_voice_precision,
            "min_non_voice_recall": args.min_non_voice_recall,
            "min_overlap": args.min_overlap,
            "min_lb95": args.min_lb95,
        },
        "readiness_pass": readiness_pass,
        "threshold_gate_pass": threshold_gate_pass,
        "lb95_gate_pass": lb95_gate_pass,
        "threshold_failures": threshold_failures,
        "lb95_failures": lb95_failures,
        "cohort_summary": cohorts,
        "failing_entries": failure_rows,
    }

    output_json = Path(args.output_json)
    output_json.parent.mkdir(parents=True, exist_ok=True)
    output_json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    lines = [
        "# Latest Radio-Play Readiness Report",
        "",
        f"- Source summary: `{summary_path}`",
        f"- Holdout tier: `{holdout_tier}`",
        f"- Readiness pass: `{readiness_pass}`",
        f"- Threshold gate pass: `{threshold_gate_pass}`",
        f"- LB95 gate pass: `{lb95_gate_pass}`",
        "",
        "## Cohort Summary",
    ]
    for cohort_name in ["modern", "legacy"]:
        c = cohorts[cohort_name]
        lines.append(
            f"- **{cohort_name}**: count={c['count']}, "
            f"precision={c['avg_non_voice_precision']:.4f}, recall={c['avg_non_voice_recall']:.4f}, "
            f"overlap={c['avg_overlap_ratio']:.4f}, "
            f"precision_lb95={c['avg_non_voice_precision_lb95']:.4f}, "
            f"recall_lb95={c['avg_non_voice_recall_lb95']:.4f}, "
            f"overlap_lb95={c['avg_overlap_ratio_lb95']:.4f}"
        )

    lines.extend(["", "## Threshold Failures"])
    lines.extend([f"- {f}" for f in threshold_failures] or ["- none"])
    lines.extend(["", "## LB95 Failures"])
    lines.extend([f"- {f}" for f in lb95_failures] or ["- none"])

    output_md = Path(args.output_md)
    output_md.parent.mkdir(parents=True, exist_ok=True)
    output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print(f"wrote readiness json: {output_json}")
    print(f"wrote readiness markdown: {output_md}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
