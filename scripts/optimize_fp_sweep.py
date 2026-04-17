#!/usr/bin/env python3
import argparse
import json
import subprocess
import time
from pathlib import Path


DEFAULT_MEDIA = [
    "testdata/raw/elephants_dream_2006.mp4",
    "testdata/raw/the_hole_1962.mp4",
    "testdata/raw/windy_day_1967.mp4",
    "testdata/raw/elephantsdream_teaser.mp4",
    "testdata/raw/caminandes_gran_dillama.mp4",
]

DEFAULT_LEGACY_MEDIA = {
    "testdata/raw/the_hole_1962.mp4",
    "testdata/raw/windy_day_1967.mp4",
}


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
                "entropy_max": None,
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
                "entropy_max": 7.2,
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
                "entropy_max": 7.4,
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
                "entropy_max": 7.6,
                "flatness_max": 0.38,
                "energy_min": 0.0015,
                "centroid_min": 200.0,
            },
        },
    ]


def expanded_candidate_matrix() -> list[dict]:
    base = candidate_matrix()

    # Focused legacy/noisy-content search space around current profile defaults.
    thresholds = [0.0125, 0.0135, 0.0150]
    min_silence = [500, 900, 1200]
    entropy_min = [3.0, 3.2, 3.4]
    entropy_max = [7.2, 7.6, 8.0]
    flatness_max = [0.38, 0.40, 0.44]
    energy_min = [0.0010, 0.0012, 0.0015]
    centroid_min = [120.0, 160.0, 200.0]

    seen = {item["name"] for item in base}
    generated = []

    for t in thresholds:
        for ms in min_silence:
            for ent in entropy_min:
                for ent_max in entropy_max:
                    for flat in flatness_max:
                        for en in energy_min:
                            for cen in centroid_min:
                                name = (
                                    f"grid_t{t:.4f}_ms{ms}_e{ent:.1f}_em{ent_max:.1f}_"
                                    f"f{flat:.2f}_en{en:.4f}_c{int(cen)}"
                                )
                                if name in seen:
                                    continue
                                generated.append(
                                    {
                                        "name": name,
                                        "extract": {
                                            "threshold": t,
                                            "min_silence_ms": ms,
                                        },
                                        "verify": {
                                            "entropy_min": ent,
                                            "entropy_max": ent_max,
                                            "flatness_max": flat,
                                            "energy_min": en,
                                            "centroid_min": cen,
                                        },
                                    }
                                )
                                seen.add(name)

    return base + generated


def cohort_for_media(media: Path, legacy_media: set[str]) -> str:
    return "legacy" if str(media) in legacy_media else "modern"


def calc_weighted_fp(entries: list[dict]) -> tuple[float, int]:
    denominator = sum(item["total_non_voice"] for item in entries)
    if denominator == 0:
        return 0.0, 0
    numerator = sum(
        item["false_positive_rate"] * item["total_non_voice"] for item in entries
    )
    return numerator / denominator, denominator


def calc_weighted_risk(entries: list[dict]) -> tuple[float, int]:
    denominator = sum(item["total_assessed_non_voice"] for item in entries)
    if denominator == 0:
        return 0.0, 0
    numerator = sum(
        item["false_positive_risk_rate"] * item["total_assessed_non_voice"]
        for item in entries
    )
    return numerator / denominator, denominator


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run FP optimization sweep with ranked candidates"
    )
    parser.add_argument(
        "--media",
        action="append",
        default=DEFAULT_MEDIA,
        help="Input media path (repeatable)",
    )
    parser.add_argument(
        "--legacy-media",
        action="append",
        default=sorted(DEFAULT_LEGACY_MEDIA),
        help="Subset treated as legacy cohort (repeatable)",
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
    parser.add_argument(
        "--expand-candidates",
        action="store_true",
        help="Enable expanded grid search candidates",
    )
    parser.add_argument(
        "--max-candidates",
        type=int,
        default=60,
        help="Maximum candidates to evaluate when --expand-candidates is enabled",
    )
    parser.add_argument(
        "--min-coverage-ratio",
        type=float,
        default=0.7,
        help="Minimum coverage vs baseline non-voice segment count",
    )
    args = parser.parse_args()

    media_paths = [Path(p) for p in args.media]
    legacy_media = {str(Path(p)) for p in args.legacy_media}
    for media_path in media_paths:
        if not media_path.exists():
            raise FileNotFoundError(f"missing media fixture: {media_path}")

    output_path = Path(args.output)
    work_dir = Path(args.work_dir)
    work_dir.mkdir(parents=True, exist_ok=True)

    sweep_results = []

    started = time.time()
    candidates = (
        expanded_candidate_matrix() if args.expand_candidates else candidate_matrix()
    )
    if args.expand_candidates:
        candidates = candidates[: max(args.max_candidates, 1)]

    for candidate in candidates:
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
            verified_count = int(summary["verified_count"])
            suspicious_count = int(summary["suspicious_count"])
            rejected_count = int(summary["rejected_count"])
            non_voice_count = verified_count + suspicious_count
            total_assessed_non_voice = (
                verified_count + suspicious_count + rejected_count
            )
            false_positive_risk_rate = (
                (suspicious_count + rejected_count) / total_assessed_non_voice
                if total_assessed_non_voice
                else 0.0
            )

            weighted_fp_num += fp_rate * non_voice_count
            weighted_fp_den += non_voice_count

            per_media.append(
                {
                    "media": str(media_path),
                    "false_positive_rate": fp_rate,
                    "false_positive_risk_rate": false_positive_risk_rate,
                    "verified_count": verified_count,
                    "suspicious_count": suspicious_count,
                    "rejected_count": rejected_count,
                    "total_non_voice": non_voice_count,
                    "total_assessed_non_voice": total_assessed_non_voice,
                }
            )

        weighted_fp = weighted_fp_num / weighted_fp_den if weighted_fp_den else 0.0
        sweep_results.append(
            {
                "candidate": candidate,
                "weighted_false_positive_rate": weighted_fp,
                "weighted_false_positive_risk_rate": 0.0,
                "evaluated_non_voice_segments": weighted_fp_den,
                "per_media": per_media,
            }
        )

    for result in sweep_results:
        risk_rate, assessed_segments = calc_weighted_risk(result["per_media"])
        result["weighted_false_positive_risk_rate"] = risk_rate
        result["assessed_non_voice_segments"] = assessed_segments

    baseline = next(
        (r for r in sweep_results if r["candidate"]["name"] == "baseline"), None
    )
    baseline_coverage = baseline["assessed_non_voice_segments"] if baseline else 0
    coverage_floor = int(baseline_coverage * args.min_coverage_ratio)

    for result in sweep_results:
        coverage = result["assessed_non_voice_segments"]
        result["coverage_ratio_vs_baseline"] = (
            (coverage / baseline_coverage) if baseline_coverage else 0.0
        )
        result["coverage_pass"] = coverage >= coverage_floor

        legacy_items = [
            item
            for item in result["per_media"]
            if cohort_for_media(Path(item["media"]), legacy_media) == "legacy"
        ]
        modern_items = [
            item
            for item in result["per_media"]
            if cohort_for_media(Path(item["media"]), legacy_media) == "modern"
        ]
        legacy_fp, legacy_count = calc_weighted_fp(legacy_items)
        legacy_risk, legacy_assessed = calc_weighted_risk(legacy_items)
        modern_fp, modern_count = calc_weighted_fp(modern_items)
        modern_risk, modern_assessed = calc_weighted_risk(modern_items)
        result["cohorts"] = {
            "legacy": {
                "weighted_false_positive_rate": legacy_fp,
                "evaluated_non_voice_segments": legacy_count,
                "weighted_false_positive_risk_rate": legacy_risk,
                "assessed_non_voice_segments": legacy_assessed,
            },
            "modern": {
                "weighted_false_positive_rate": modern_fp,
                "evaluated_non_voice_segments": modern_count,
                "weighted_false_positive_risk_rate": modern_risk,
                "assessed_non_voice_segments": modern_assessed,
            },
        }

    ranked = sorted(
        sweep_results,
        key=lambda r: (
            0 if r["coverage_pass"] else 1,
            r["weighted_false_positive_risk_rate"],
            r["weighted_false_positive_rate"],
        ),
    )

    coverage_pass_candidates = [r for r in ranked if r["coverage_pass"]]
    ranked_legacy = sorted(
        coverage_pass_candidates,
        key=lambda r: r["cohorts"]["legacy"]["weighted_false_positive_risk_rate"],
    )
    ranked_modern = sorted(
        coverage_pass_candidates,
        key=lambda r: r["cohorts"]["modern"]["weighted_false_positive_risk_rate"],
    )

    final_report = {
        "generated_at_unix": int(time.time()),
        "elapsed_ms": int((time.time() - started) * 1000),
        "coverage_guard": {
            "min_coverage_ratio": args.min_coverage_ratio,
            "baseline_non_voice_segments": baseline_coverage,
            "coverage_floor": coverage_floor,
        },
        "ranked_candidates": ranked,
        "best_candidate": ranked[0] if ranked else None,
        "best_per_cohort": {
            "legacy": ranked_legacy[0] if ranked_legacy else None,
            "modern": ranked_modern[0] if ranked_modern else None,
        },
        "recommended_policy": {
            "legacy_candidate": ranked_legacy[0]["candidate"]["name"]
            if ranked_legacy
            else None,
            "modern_candidate": ranked_modern[0]["candidate"]["name"]
            if ranked_modern
            else None,
            "note": "Use per-cohort candidates when modern and legacy optima diverge.",
        },
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(final_report, indent=2) + "\n", encoding="utf-8")
    print(f"wrote ranked sweep report: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
