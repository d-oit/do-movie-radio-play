#!/usr/bin/env python3
"""Build the consolidated radio-play readiness report.

The report keeps two scoring paths apart on purpose:

* ``detector_only`` - the VAD timeline scored against truth, no cues.
* ``cue_assisted`` - the shipped path (detector + subtitle/Whisper cues, cue
  padding, gap subtraction and narration placement), read from a cue-path sweep.

Pooled cohort averages are reported next to the worst entry so a single failing
source can never hide behind a healthy mean.
"""

import argparse
import json
import math
import sys
from pathlib import Path


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
    legacy_markers = ["the_hole_1962", "windy_day_1967", "plan9_1959", "charade_1963"]
    return (
        "legacy" if any(marker in normalized for marker in legacy_markers) else "modern"
    )


def load_manifest_metadata(path: Path) -> dict:
    if not path.exists():
        return {}
    payload = json.loads(path.read_text(encoding="utf-8"))
    metadata = {}
    for entry in payload.get("entries", []):
        entry_id = entry.get("id")
        if not isinstance(entry_id, str):
            continue
        metadata[entry_id] = {
            "cohort": entry.get("cohort"),
            "language": entry.get("language"),
            "vad_engine": entry.get("vad_engine"),
            "cue_source": entry.get("cue_source", "detector_only"),
            "truth_source": entry.get("truth_source"),
        }
    return metadata


def entry_metadata(result: dict, manifest: dict) -> dict:
    entry_id = result.get("id", "unknown")
    media = str(result.get("input_media") or entry_id)
    declared = manifest.get(entry_id, {})
    return {
        "cohort": result.get("cohort")
        or declared.get("cohort")
        or cohort_for_media(media),
        "language": result.get("language") or declared.get("language") or "und",
        "vad_engine": result.get("vad_engine") or declared.get("vad_engine") or "unknown",
        "cue_source": result.get("cue_source")
        or declared.get("cue_source")
        or "detector_only",
        "truth_source": result.get("truth_source")
        or declared.get("truth_source")
        or "unknown",
    }


def avg(rows: list[dict], key: str) -> float:
    if not rows:
        return 0.0
    return sum(float(r[key]) for r in rows) / len(rows)


def cohort_summary(rows: list[dict]) -> dict:
    worst = min(rows, key=lambda r: r["overlap_ratio"]) if rows else None
    return {
        "count": len(rows),
        "languages": sorted({r["language"] for r in rows}),
        "avg_overlap_ratio": avg(rows, "overlap_ratio"),
        "avg_non_voice_precision": avg(rows, "non_voice_precision"),
        "avg_non_voice_recall": avg(rows, "non_voice_recall"),
        "avg_overlap_ratio_lb95": avg(rows, "overlap_ratio_lb95"),
        "avg_non_voice_precision_lb95": avg(rows, "non_voice_precision_lb95"),
        "avg_non_voice_recall_lb95": avg(rows, "non_voice_recall_lb95"),
        "worst_entry": worst["id"] if worst else None,
        "worst_overlap_ratio": worst["overlap_ratio"] if worst else 0.0,
        "failing_entries": [r["id"] for r in rows if r.get("failing")],
    }


def cue_assisted_section(cue_summary_path: Path) -> dict:
    if not cue_summary_path.exists():
        return {"available": False, "reason": f"missing {cue_summary_path}"}
    payload = json.loads(cue_summary_path.read_text(encoding="utf-8"))
    rows = []
    for item in payload.get("holdout", []):
        metrics = item.get("metrics", {})
        languages = sorted(
            lang for lang in (item.get("per_language") or {}) if isinstance(lang, str)
        )
        rows.append(
            {
                "id": item.get("id", "unknown"),
                "language": item.get("language")
                or ("+".join(languages) if languages else "und"),
                "cue_source": item.get("source", "unknown"),
                "scoring_mode": metrics.get("scoring_mode", "detector_plus_cues"),
                "dialogue_leak_ratio": metrics.get("dialogue_leak_ratio"),
                "narratable_retained_ratio": metrics.get("narratable_retained_ratio"),
                "cue_segment_precision": metrics.get("cue_segment_precision"),
                "dialogue_segment_recall": metrics.get("dialogue_segment_recall"),
                "missed_short_utterances": metrics.get("missed_short_utterances"),
                "missed_shouted_utterances": metrics.get("missed_shouted_utterances"),
            }
        )
    return {
        "available": True,
        "source": str(cue_summary_path),
        "shipped_config": payload.get("shipped_config"),
        "pending": payload.get("pending", []),
        "entries": rows,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build a consolidated radio-play readiness report"
    )
    parser.add_argument(
        "--summary",
        default="analysis/validation/full-sweep-summary.json",
        help="Validation summary JSON",
    )
    parser.add_argument(
        "--manifest",
        default="testdata/validation/radio-play-manifest.json",
        help="Validation manifest supplying cohort/language/engine metadata",
    )
    parser.add_argument(
        "--cue-summary",
        default="analysis/validation/cue-path-sweep.json",
        help="Optional cue-path sweep JSON for the cue-assisted section",
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
    parser.add_argument(
        "--require-pass",
        action="store_true",
        help="Exit non-zero when consolidated readiness does not pass",
    )
    args = parser.parse_args()

    summary_path = Path(args.summary)
    if not summary_path.exists():
        raise FileNotFoundError(f"missing summary file: {summary_path}")

    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    manifest = load_manifest_metadata(Path(args.manifest))
    holdout_tier = args.holdout_tier.upper()
    results = summary.get("results", [])
    holdout = [r for r in results if str(r.get("tier", "")).upper() == holdout_tier]

    threshold_failures = []
    readiness_pass = True

    if not holdout:
        print(f"WARN: no holdout entries found for tier={holdout_tier}", file=sys.stderr)
        threshold_failures.append(
            f"HOLDING_TIER_{holdout_tier}: missing (all entries skipped or manifest empty)"
        )
        readiness_pass = False

    lb95_failures = []
    failure_rows = []
    cohort_rows: dict[str, list[dict]] = {}

    for item in holdout:
        metrics = item.get("metrics", {})
        entry_id = item.get("id", "unknown")
        media = str(item.get("input_media") or entry_id)
        meta = entry_metadata(item, manifest)

        precision = metric_value(metrics, "non_voice_precision")
        recall = metric_value(metrics, "non_voice_recall")
        overlap = metric_value(metrics, "overlap_ratio")
        expected_segments = int(metrics.get("expected_segments") or 0)
        predicted_segments = int(metrics.get("predicted_segments") or 0)
        expected_n = sample_size_from_ms(
            metrics, "non_voice_expected_ms", expected_segments
        )
        predicted_n = sample_size_from_ms(
            metrics, "non_voice_predicted_ms", predicted_segments
        )

        precision_lb95 = wilson_lower_bound(precision, predicted_n)
        recall_lb95 = wilson_lower_bound(recall, expected_n)
        overlap_lb95 = wilson_lower_bound(overlap, expected_n)

        row = {
            "id": entry_id,
            "media": media,
            "tier": item.get("tier", "unknown"),
            "cohort": meta["cohort"],
            "language": meta["language"],
            "vad_engine": meta["vad_engine"],
            "cue_source": meta["cue_source"],
            "truth_source": meta["truth_source"],
            "non_voice_precision": precision,
            "non_voice_recall": recall,
            "overlap_ratio": overlap,
            "non_voice_precision_lb95": precision_lb95,
            "non_voice_recall_lb95": recall_lb95,
            "overlap_ratio_lb95": overlap_lb95,
            "expected_segments": expected_segments,
            "predicted_segments": predicted_segments,
            "expected_non_voice_samples": expected_n,
            "predicted_non_voice_samples": predicted_n,
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

        row["failing"] = (
            precision < args.min_non_voice_precision
            or recall < args.min_non_voice_recall
            or overlap < args.min_overlap
            or precision_lb95 < args.min_lb95
            or recall_lb95 < args.min_lb95
            or overlap_lb95 < args.min_lb95
        )
        if row["failing"]:
            failure_rows.append(row)

        cohort_rows.setdefault(row["cohort"], []).append(row)

    cohorts = {name: cohort_summary(rows) for name, rows in cohort_rows.items()}
    for name in ("modern", "legacy"):
        cohorts.setdefault(name, cohort_summary([]))

    threshold_gate_pass = (len(threshold_failures) == 0) if holdout else False
    lb95_gate_pass = (len(lb95_failures) == 0) if holdout else False
    readiness_pass = readiness_pass and threshold_gate_pass and lb95_gate_pass

    cue_assisted = cue_assisted_section(Path(args.cue_summary))

    report = {
        "summary": str(summary_path),
        "manifest": str(args.manifest),
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
        "detector_only": {
            "entries": [row for rows in cohort_rows.values() for row in rows],
            "cohort_summary": cohorts,
        },
        "cue_assisted": cue_assisted,
        "failing_entries": failure_rows,
        "pending": summary.get("pending", []),
    }

    output_json = Path(args.output_json)
    output_json.parent.mkdir(parents=True, exist_ok=True)
    output_json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    lines = [
        "# Latest Radio-Play Readiness Report",
        "",
        f"- Source summary: `{summary_path}`",
        f"- Manifest: `{args.manifest}`",
        f"- Holdout tier: `{holdout_tier}`",
        f"- Readiness pass: `{readiness_pass}`",
        f"- Threshold gate pass: `{threshold_gate_pass}`",
        f"- LB95 gate pass: `{lb95_gate_pass}`",
        "",
        "## Detector-Only Cohort Summary",
    ]
    for cohort_name in ("modern", "legacy"):
        c = cohorts[cohort_name]
        lines.append(
            f"- **{cohort_name}**: count={c['count']}, languages={','.join(c['languages']) or 'none'}, "
            f"precision={c['avg_non_voice_precision']:.4f}, recall={c['avg_non_voice_recall']:.4f}, "
            f"overlap={c['avg_overlap_ratio']:.4f}, "
            f"precision_lb95={c['avg_non_voice_precision_lb95']:.4f}, "
            f"recall_lb95={c['avg_non_voice_recall_lb95']:.4f}, "
            f"overlap_lb95={c['avg_overlap_ratio_lb95']:.4f}, "
            f"worst={c['worst_entry'] or 'none'} ({c['worst_overlap_ratio']:.4f})"
        )

    lines.extend(
        [
            "",
            "## Detector-Only Per-Film",
            "",
            "| Entry | Cohort | Lang | VAD | Precision | Recall | Overlap | Overlap LB95 |",
            "|---|---|---|---|---:|---:|---:|---:|",
        ]
    )
    for row in report["detector_only"]["entries"]:
        lines.append(
            f"| `{row['id']}` | {row['cohort']} | {row['language']} | {row['vad_engine']} | "
            f"{row['non_voice_precision']:.4f} | {row['non_voice_recall']:.4f} | "
            f"{row['overlap_ratio']:.4f} | {row['overlap_ratio_lb95']:.4f} |"
        )

    lines.extend(["", "## Threshold Failures"])
    lines.extend([f"- {f}" for f in threshold_failures] or ["- none"])
    lines.extend(["", "## LB95 Failures"])
    lines.extend([f"- {f}" for f in lb95_failures] or ["- none"])

    lines.extend(["", "## Cue-Assisted (Shipped Path)"])
    if cue_assisted.get("available"):
        lines.append(f"- Source: `{cue_assisted['source']}`")
        for row in cue_assisted["entries"]:
            lines.append(
                f"- `{row['id']}` ({row['language']}, {row['cue_source']}, {row['scoring_mode']}): "
                f"dialogue_leak={row['dialogue_leak_ratio']}, "
                f"narratable_retained={row['narratable_retained_ratio']}, "
                f"missed_short={row['missed_short_utterances']}, "
                f"missed_shouted={row['missed_shouted_utterances']}"
            )
        if cue_assisted.get("pending"):
            lines.append(
                f"- Pending assets: {', '.join(str(p.get('id')) for p in cue_assisted['pending'])}"
            )
    else:
        lines.append(f"- Not available: {cue_assisted.get('reason')}")

    if report["pending"]:
        lines.extend(["", "## Pending Sources"])
        for item in report["pending"]:
            lines.append(
                f"- `{item.get('id')}` ({item.get('cohort')}, {item.get('language')}): "
                f"{item.get('status')} - {item.get('reason') or 'no reason recorded'}"
            )

    output_md = Path(args.output_md)
    output_md.parent.mkdir(parents=True, exist_ok=True)
    output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print(f"wrote readiness json: {output_json}")
    print(f"wrote readiness markdown: {output_md}")
    if args.require_pass and not readiness_pass:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
