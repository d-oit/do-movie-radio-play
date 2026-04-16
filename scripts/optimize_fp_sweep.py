#!/usr/bin/env python3
import argparse
import json
import subprocess
import time
from pathlib import Path


def run(cmd: list[str]) -> None:
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(
            f"command failed: {' '.join(cmd)}\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )


def media_key(path: Path) -> str:
    return path.stem.replace(".", "_").replace("-", "_")


def candidate_matrix() -> list[dict]:
    return [
        {
            "name": "baseline",
            "extract": {
                "threshold": None,
                "min_silence_ms": None,
            },
            "verify": {
                "entropy_min": None,
                "flatness_max": None,
                "energy_min": None,
                "centroid_min": None,
            },
        },
        {
            "name": "low_threshold",
            "extract": {
                "threshold": 0.0135,
                "min_silence_ms": None,
            },
            "verify": {
                "entropy_min": 3.4,
                "flatness_max": 0.44,
                "energy_min": 0.001,
                "centroid_min": 120.0,
            },
        },
        {
            "name": "legacy_guard_v1",
            "extract": {
                "threshold": 0.0135,
                "min_silence_ms": 1200,
            },
            "verify": {
                "entropy_min": 3.2,
                "flatness_max": 0.4,
                "energy_min": 0.0012,
                "centroid_min": 160.0,
            },
        },
        {
            "name": "legacy_guard_v2",
            "extract": {
                "threshold": 0.0125,
                "min_silence_ms": 1800,
            },
            "verify": {
                "entropy_min": 3.0,
                "flatness_max": 0.38,
                "energy_min": 0.0015,
                "centroid_min": 200.0,
            },
        },
    ]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run FP optimization sweep with ranked candidates"
    )
    parser.add_argument(
        "--media",
        action="append",
        default=[
            "testdata/raw/elephants_dream_2006.mp4",
            "testdata/raw/the_hole_1962.mp4",
            "testdata/raw/windy_day_1967.mp4",
            "testdata/raw/elephantsdream_teaser.mp4",
            "testdata/raw/caminandes_gran_dillama.mp4",
        ],
        help="Input media path (repeatable)",
    )
    parser.add_argument(
        "--output",
        default="analysis/optimization/fp-sweep-ranked.json",
        help="Output ranked JSON report",
    )
    parser.add_argument(
        "--work-dir",
        default="analysis/optimization/fp-sweep-runs",
        help="Directory for per-candidate artifacts",
    )
    args = parser.parse_args()

    media_paths = [Path(p) for p in args.media]
    for media_path in media_paths:
        if not media_path.exists():
            raise FileNotFoundError(f"missing media fixture: {media_path}")

    output_path = Path(args.output)
    work_dir = Path(args.work_dir)
    work_dir.mkdir(parents=True, exist_ok=True)

    sweep_results = []

    started = time.time()
    for candidate in candidate_matrix():
        candidate_name = candidate["name"]
        candidate_dir = work_dir / candidate_name
        candidate_dir.mkdir(parents=True, exist_ok=True)

        per_media = []
        weighted_fp_num = 0.0
        weighted_fp_den = 0

        for media_path in media_paths:
            key = media_key(media_path)
            extract_out = candidate_dir / f"{key}.timeline.json"
            verify_out = candidate_dir / f"{key}.verified.json"

            extract_cmd = [
                "cargo",
                "run",
                "--quiet",
                "--bin",
                "timeline",
                "--",
                "extract",
                str(media_path),
                "--output",
                str(extract_out),
                "--config",
                "config/profiles/radio-play.json",
                "--vad-engine",
                "spectral",
            ]
            if candidate["extract"]["threshold"] is not None:
                extract_cmd.extend(
                    ["--threshold", str(candidate["extract"]["threshold"])]
                )
            if candidate["extract"]["min_silence_ms"] is not None:
                extract_cmd.extend(
                    ["--min-silence-ms", str(candidate["extract"]["min_silence_ms"])]
                )

            run(extract_cmd)

            verify_cmd = [
                "cargo",
                "run",
                "--quiet",
                "--bin",
                "timeline",
                "--",
                "verify-timeline",
                str(media_path),
                "--timeline",
                str(extract_out),
                "--output",
                str(verify_out),
            ]
            for flag, value in candidate["verify"].items():
                if value is not None:
                    verify_cmd.extend([f"--{flag.replace('_', '-')}", str(value)])

            run(verify_cmd)

            report = json.loads(verify_out.read_text(encoding="utf-8"))
            summary = report["summary"]
            fp_rate = float(summary["false_positive_rate"])
            non_voice_count = int(
                summary["verified_count"] + summary["suspicious_count"]
            )

            weighted_fp_num += fp_rate * non_voice_count
            weighted_fp_den += non_voice_count

            per_media.append(
                {
                    "media": str(media_path),
                    "false_positive_rate": fp_rate,
                    "verified_count": int(summary["verified_count"]),
                    "suspicious_count": int(summary["suspicious_count"]),
                    "total_non_voice": non_voice_count,
                }
            )

        weighted_fp = weighted_fp_num / weighted_fp_den if weighted_fp_den else 0.0
        sweep_results.append(
            {
                "candidate": candidate,
                "weighted_false_positive_rate": weighted_fp,
                "evaluated_non_voice_segments": weighted_fp_den,
                "per_media": per_media,
            }
        )

    ranked = sorted(sweep_results, key=lambda r: r["weighted_false_positive_rate"])

    final_report = {
        "generated_at_unix": int(time.time()),
        "elapsed_ms": int((time.time() - started) * 1000),
        "ranked_candidates": ranked,
        "best_candidate": ranked[0] if ranked else None,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(final_report, indent=2) + "\n", encoding="utf-8")
    print(f"wrote ranked sweep report: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
