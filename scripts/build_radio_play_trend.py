#!/usr/bin/env python3
"""Append a sweep to the radio-play trend history and render a concise trend.

The weekly job publishes this artifact so a slow drift is visible without
diffing whole sweep summaries. Pending sources and unavailable optional
dependencies are recorded in the same artifact instead of being dropped.
"""

import argparse
import json
from datetime import datetime, timezone
from pathlib import Path

METRICS = ("overlap_ratio", "non_voice_precision", "non_voice_recall")


def load_history(path: Path) -> dict:
    if not path.exists():
        return {"version": 1, "points": []}
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload.get("points"), list):
        raise ValueError(f"{path}: 'points' must be a list")
    return payload


def point_from_summary(summary: dict, run_id: str, notes: list[str]) -> dict:
    entries = []
    for result in summary.get("results", []):
        metrics = result.get("metrics", {})
        entries.append(
            {
                "id": result.get("id", "unknown"),
                "cohort": result.get("cohort"),
                "language": result.get("language"),
                "vad_engine": result.get("vad_engine"),
                "cue_source": result.get("cue_source", "detector_only"),
                **{key: metrics.get(key) for key in METRICS},
            }
        )
    return {
        "run": run_id,
        "recorded_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "entry_count": len(entries),
        "pending": summary.get("pending", []),
        "notes": notes,
        "entries": entries,
    }


def previous_point(history: dict, run_id: str) -> dict | None:
    for point in reversed(history["points"]):
        if point.get("run") != run_id:
            return point
    return None


def delta(current: float | None, previous: float | None) -> str:
    if current is None or previous is None:
        return "-"
    return f"{current - previous:+.4f}"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Append a validation sweep to the radio-play trend history"
    )
    parser.add_argument(
        "--summary",
        default="analysis/validation/radio-play-sweep-summary.json",
        help="Current sweep summary JSON",
    )
    parser.add_argument(
        "--history",
        default="analysis/validation/radio-play-trend.json",
        help="Trend history JSON (created when absent)",
    )
    parser.add_argument(
        "--output-md",
        default="analysis/learnings/latest-radio-play-trend.md",
        help="Markdown trend output",
    )
    parser.add_argument("--run-id", help="Run label; defaults to the UTC date")
    parser.add_argument("--max-points", type=int, default=26)
    parser.add_argument(
        "--note",
        action="append",
        default=[],
        help="Note about unavailable assets or optional dependencies (repeatable)",
    )
    args = parser.parse_args()

    summary_path = Path(args.summary)
    if not summary_path.exists():
        raise FileNotFoundError(f"missing summary file: {summary_path}")
    summary = json.loads(summary_path.read_text(encoding="utf-8"))

    run_id = args.run_id or datetime.now(timezone.utc).strftime("%Y-%m-%d")
    history_path = Path(args.history)
    history = load_history(history_path)
    previous = previous_point(history, run_id)

    point = point_from_summary(summary, run_id, args.note)
    history["points"] = [p for p in history["points"] if p.get("run") != run_id]
    history["points"].append(point)
    history["points"] = history["points"][-args.max_points :]
    history_path.parent.mkdir(parents=True, exist_ok=True)
    history_path.write_text(json.dumps(history, indent=2) + "\n", encoding="utf-8")

    previous_entries = {
        entry["id"]: entry for entry in (previous or {}).get("entries", [])
    }
    lines = [
        "# Radio-Play Validation Trend",
        "",
        f"- Run: `{run_id}`",
        f"- Entries: {point['entry_count']}",
        f"- History points: {len(history['points'])}",
        "",
        "| Entry | Cohort | Lang | Overlap | Δ | Precision | Δ | Recall | Δ |",
        "|---|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    for entry in point["entries"]:
        prior = previous_entries.get(entry["id"], {})
        lines.append(
            f"| `{entry['id']}` | {entry.get('cohort') or '-'} | {entry.get('language') or '-'} | "
            f"{entry.get('overlap_ratio')} | {delta(entry.get('overlap_ratio'), prior.get('overlap_ratio'))} | "
            f"{entry.get('non_voice_precision')} | {delta(entry.get('non_voice_precision'), prior.get('non_voice_precision'))} | "
            f"{entry.get('non_voice_recall')} | {delta(entry.get('non_voice_recall'), prior.get('non_voice_recall'))} |"
        )

    if point["pending"]:
        lines.extend(["", "## Pending Sources"])
        for item in point["pending"]:
            lines.append(
                f"- `{item.get('id')}` ({item.get('cohort')}, {item.get('language')}): "
                f"{item.get('status')} - {item.get('reason') or 'no reason recorded'}"
            )
    if point["notes"]:
        lines.extend(["", "## Notes"])
        lines.extend([f"- {note}" for note in point["notes"]])

    output_md = Path(args.output_md)
    output_md.parent.mkdir(parents=True, exist_ok=True)
    output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print(f"wrote trend history: {history_path}")
    print(f"wrote trend markdown: {output_md}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
