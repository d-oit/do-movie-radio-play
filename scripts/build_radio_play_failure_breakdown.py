#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def cohort_for_media(media_path: str) -> str:
    normalized = media_path.lower()
    legacy_markers = ["the_hole_1962", "windy_day_1967", "plan9_1959", "charade_1963"]
    return "legacy" if any(m in normalized for m in legacy_markers) else "modern"


def load_manifest_metadata(path: Path) -> dict:
    if not path.exists():
        return {}
    payload = json.loads(path.read_text(encoding="utf-8"))
    return {
        entry.get("id"): entry
        for entry in payload.get("entries", [])
        if isinstance(entry.get("id"), str)
    }


def metric(metrics: dict, key: str) -> float:
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


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build compact radio-play failure breakdown from validation summary"
    )
    parser.add_argument(
        "--summary",
        default="analysis/validation/full-sweep-summary.json",
        help="Validation summary JSON",
    )
    parser.add_argument(
        "--manifest",
        default="testdata/validation/radio-play-manifest.json",
        help="Validation manifest supplying cohort/language metadata",
    )
    parser.add_argument(
        "--output-json",
        default="analysis/validation/radio-play-failure-breakdown.json",
        help="Output JSON path",
    )
    parser.add_argument(
        "--output-md",
        default="analysis/learnings/latest-radio-play-failure-breakdown.md",
        help="Output markdown summary path",
    )
    args = parser.parse_args()

    summary_path = Path(args.summary)
    if not summary_path.exists():
        raise FileNotFoundError(f"missing summary file: {summary_path}")

    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    manifest = load_manifest_metadata(Path(args.manifest))
    results = summary.get("results", [])

    cohorts = {"modern": [], "legacy": []}
    failures = []
    for result in results:
        media = str(result.get("input_media") or result.get("id") or "unknown")
        declared = manifest.get(result.get("id"), {})
        c = result.get("cohort") or declared.get("cohort") or cohort_for_media(media)
        metrics = result.get("metrics", {})
        row = {
            "id": result.get("id", "unknown"),
            "tier": result.get("tier", "unknown"),
            "media": media,
            "cohort": c,
            "language": result.get("language") or declared.get("language") or "und",
            "vad_engine": result.get("vad_engine")
            or declared.get("vad_engine")
            or "unknown",
            "overlap_ratio": metric(metrics, "overlap_ratio"),
            "non_voice_precision": metric(metrics, "non_voice_precision"),
            "non_voice_recall": metric(metrics, "non_voice_recall"),
        }
        cohorts.setdefault(c, []).append(row)

        if (
            row["overlap_ratio"] < 0.95
            or row["non_voice_precision"] < 0.95
            or row["non_voice_recall"] < 0.95
        ):
            failures.append(row)

    def avg(rows: list[dict], key: str) -> float:
        if not rows:
            return 0.0
        return sum(float(r[key]) for r in rows) / len(rows)

    cohort_summary = {
        name: {
            "count": len(rows),
            "languages": sorted({r["language"] for r in rows}),
            "avg_overlap_ratio": avg(rows, "overlap_ratio"),
            "avg_non_voice_precision": avg(rows, "non_voice_precision"),
            "avg_non_voice_recall": avg(rows, "non_voice_recall"),
            "worst_entry": min(rows, key=lambda r: r["overlap_ratio"])["id"]
            if rows
            else None,
        }
        for name, rows in cohorts.items()
    }
    for name in ("modern", "legacy"):
        cohort_summary.setdefault(
            name,
            {
                "count": 0,
                "languages": [],
                "avg_overlap_ratio": 0.0,
                "avg_non_voice_precision": 0.0,
                "avg_non_voice_recall": 0.0,
                "worst_entry": None,
            },
        )

    output = {
        "summary": str(summary_path),
        "entry_count": len(results),
        "failure_count": len(failures),
        "cohort_summary": cohort_summary,
        "failures": failures,
    }

    output_json_path = Path(args.output_json)
    output_json_path.parent.mkdir(parents=True, exist_ok=True)
    output_json_path.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")

    lines = [
        "# Latest Radio-Play Failure Breakdown",
        "",
        f"- Source summary: `{summary_path}`",
        f"- Entries analyzed: `{len(results)}`",
        f"- Failing entries (<0.95 on overlap/precision/recall): `{len(failures)}`",
        "",
        "## Cohort Averages",
    ]
    for cohort_name in ["modern", "legacy"]:
        c = cohort_summary[cohort_name]
        lines.extend(
            [
                f"- **{cohort_name}**: count={c['count']}, "
                f"languages={','.join(c['languages']) or 'none'}, "
                f"overlap={c['avg_overlap_ratio']:.4f}, "
                f"precision={c['avg_non_voice_precision']:.4f}, "
                f"recall={c['avg_non_voice_recall']:.4f}, "
                f"worst={c['worst_entry'] or 'none'}",
            ]
        )

    lines.extend(["", "## Top Failures"])
    for row in failures[:10]:
        lines.append(
            f"- `{row['id']}` ({row['cohort']}, {row['language']}) "
            f"overlap={row['overlap_ratio']:.4f}, "
            f"precision={row['non_voice_precision']:.4f}, recall={row['non_voice_recall']:.4f}"
        )

    output_md_path = Path(args.output_md)
    output_md_path.parent.mkdir(parents=True, exist_ok=True)
    output_md_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print(f"wrote breakdown json: {output_json_path}")
    print(f"wrote breakdown markdown: {output_md_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
