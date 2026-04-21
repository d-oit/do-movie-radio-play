#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def find_media_metrics(report: dict, media_path: str) -> dict | None:
    ranked = report.get("ranked_candidates") or []
    if not ranked:
        return None
    best = ranked[0]
    per_media = best.get("per_media") or []
    for row in per_media:
        if row.get("media") == media_path:
            return row
    return None


def ms(value: int) -> str:
    return f"{int(value)}ms"


def pct(value: float) -> str:
    return f"{float(value) * 100.0:.2f}%"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build compact CI summary for extra modern movie artifacts"
    )
    parser.add_argument(
        "--benchmark",
        action="append",
        required=True,
        help="Benchmark JSON file (repeatable)",
    )
    parser.add_argument(
        "--fp-sweep",
        required=True,
        help="FP sweep JSON report path",
    )
    parser.add_argument(
        "--output-md",
        default="analysis/optimization/modern-extra-ci-summary.md",
        help="Output markdown summary path",
    )
    parser.add_argument(
        "--max-fp-rate",
        type=float,
        default=None,
        help="Optional hard gate: fail if any listed media fp_rate exceeds this ratio",
    )
    parser.add_argument(
        "--fail-on-missing-media",
        action="store_true",
        help="Fail when a benchmark media row is missing in the FP sweep report",
    )
    args = parser.parse_args()

    bench_files = [Path(p) for p in args.benchmark]
    sweep = load_json(Path(args.fp_sweep))

    lines = [
        "# Modern Extra CI Summary",
        "",
        "## Benchmarks",
        "",
        "| media | total | decode | frame | segments |",
        "|---|---:|---:|---:|---:|",
    ]

    media_paths: list[str] = []
    for bench_file in bench_files:
        bench = load_json(bench_file)
        media = str(bench.get("input_file") or bench_file.name)
        media_paths.append(media)
        stage = bench.get("stage_ms") or {}
        lines.append(
            "| {media} | {total} | {decode} | {frame} | {segments} |".format(
                media=media,
                total=ms(int(bench.get("total_ms") or 0)),
                decode=ms(int(stage.get("decode_ms") or 0)),
                frame=ms(int(stage.get("frame_ms") or 0)),
                segments=int(bench.get("segment_count") or 0),
            )
        )

    lines.extend(["", "## FP Eval (Best Candidate)", ""])
    ranked = sweep.get("ranked_candidates") or []
    failures: list[str] = []
    if not ranked:
        lines.append("No ranked candidates found.")
        if args.max_fp_rate is not None:
            failures.append("no ranked candidates found for FP gate")
    else:
        best = ranked[0]
        candidate_name = best.get("candidate", {}).get("name", "unknown")
        lines.append(f"Best candidate: `{candidate_name}`")
        lines.append("")
        lines.append("| media | fp_rate | suspicious | total_non_voice |")
        lines.append("|---|---:|---:|---:|")
        for media in media_paths:
            row = find_media_metrics(sweep, media)
            if row is None:
                lines.append(f"| {media} | n/a | n/a | n/a |")
                if args.fail_on_missing_media or args.max_fp_rate is not None:
                    failures.append(f"missing fp metrics for media: {media}")
                continue
            fp_rate = float(row.get("false_positive_rate") or 0.0)
            lines.append(
                "| {media} | {fp} | {sus} | {total} |".format(
                    media=media,
                    fp=pct(fp_rate),
                    sus=int(row.get("suspicious_count") or 0),
                    total=int(row.get("total_non_voice") or 0),
                )
            )
            if args.max_fp_rate is not None and fp_rate > args.max_fp_rate:
                failures.append(
                    f"{media}: fp_rate={fp_rate:.6f} exceeds limit {args.max_fp_rate:.6f}"
                )

    if args.max_fp_rate is not None:
        lines.extend(["", "## Gate", ""])
        if failures:
            lines.append("Status: FAIL")
            for failure in failures:
                lines.append(f"- {failure}")
        else:
            lines.append(f"Status: PASS (max_fp_rate={args.max_fp_rate:.6f})")

    output = Path(args.output_md)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote modern ci summary: {output}")
    if failures:
        for failure in failures:
            print(f"gate failure: {failure}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
