#!/usr/bin/env python3
import argparse
import itertools
import json
import subprocess
import tempfile
import time
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
    return 0.0


def harmonic_mean(a: float, b: float) -> float:
    if a <= 0.0 or b <= 0.0:
        return 0.0
    return 2.0 * a * b / (a + b)


def harmonic_mean3(a: float, b: float, c: float) -> float:
    if a <= 0.0 or b <= 0.0 or c <= 0.0:
        return 0.0
    return 3.0 / ((1.0 / a) + (1.0 / b) + (1.0 / c))


def baseline_metrics(summary_path: Path, entry_ids: set[str]) -> dict[str, dict]:
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    indexed = {entry["id"]: entry for entry in summary.get("results", [])}
    missing = [entry_id for entry_id in entry_ids if entry_id not in indexed]
    if missing:
        raise ValueError(f"missing baseline entries in summary: {', '.join(missing)}")

    baselines: dict[str, dict] = {}
    for entry_id in entry_ids:
        metrics = indexed[entry_id].get("metrics", {})
        baselines[entry_id] = {
            "non_voice_precision": metric_value(metrics, "non_voice_precision"),
            "non_voice_recall": metric_value(metrics, "non_voice_recall"),
            "overlap_ratio": float(metrics.get("overlap_ratio") or 0.0),
        }
    return baselines


def candidates(search_mode: str) -> list[dict]:
    energy = [0.010, 0.0125, 0.015]
    min_non_voice_ms = [300, 500, 800, 1000, 1500, 2500, 4000]
    min_speech_ms = [200, 300, 500, 800, 1200, 1800]
    entropy_min = [2.6, 3.0]
    flatness_max = [0.32, 0.38]
    merge_gap_ms = [200, 250, 400]
    merge_min_gap = [300, 400, 600]
    merge_min_silence = [200, 300, 500, 800]
    merge_min_speech_duration = [300, 500, 800, 1200, 1800, 2500]
    merge_strategy = ["all", "sparse"]

    entropy_max = [7.4, 7.8]
    centroid_min = [120.0, 180.0]
    centroid_max = [2800.0, 3800.0]

    generated = []
    if search_mode == "extended":
        grid = itertools.product(
            energy,
            min_non_voice_ms,
            min_speech_ms,
            entropy_min,
            flatness_max,
            merge_gap_ms,
            merge_min_gap,
            merge_min_silence,
            merge_min_speech_duration,
            merge_strategy,
            entropy_max,
            centroid_min,
            centroid_max,
        )
        for (
            t,
            nv,
            sp,
            ent,
            flat,
            mg,
            mmg,
            msi,
            mspd,
            strat,
            ent_max,
            cmin,
            cmax,
        ) in grid:
            generated.append(
                {
                    "name": (
                        f"t{t:.4f}_nv{nv}_sp{sp}_ent{ent:.1f}_em{ent_max:.1f}"
                        f"_flat{flat:.2f}_cmin{int(cmin)}_cmax{int(cmax)}"
                        f"_mg{mg}_mmg{mmg}_msi{msi}_mspd{mspd}_ms{strat}"
                    ),
                    "energy_threshold": t,
                    "min_non_voice_ms": nv,
                    "min_speech_ms": sp,
                    "spectral_entropy_min": ent,
                    "spectral_entropy_max": ent_max,
                    "spectral_flatness_max": flat,
                    "spectral_centroid_min": cmin,
                    "spectral_centroid_max": cmax,
                    "merge_gap_ms": mg,
                    "merge_min_gap_to_merge": mmg,
                    "merge_min_silence_duration": msi,
                    "merge_min_speech_duration": mspd,
                    "merge_strategy": strat,
                }
            )
    else:
        for t, nv, sp, ent, flat, mg, mmg, msi, mspd, strat in itertools.product(
            energy,
            min_non_voice_ms,
            min_speech_ms,
            entropy_min,
            flatness_max,
            merge_gap_ms,
            merge_min_gap,
            merge_min_silence,
            merge_min_speech_duration,
            merge_strategy,
        ):
            generated.append(
                {
                    "name": (
                        f"t{t:.4f}_nv{nv}_sp{sp}_ent{ent:.1f}_flat{flat:.2f}"
                        f"_mg{mg}_mmg{mmg}_msi{msi}_mspd{mspd}_ms{strat}"
                    ),
                    "energy_threshold": t,
                    "min_non_voice_ms": nv,
                    "min_speech_ms": sp,
                    "spectral_entropy_min": ent,
                    "spectral_flatness_max": flat,
                    "merge_gap_ms": mg,
                    "merge_min_gap_to_merge": mmg,
                    "merge_min_silence_duration": msi,
                    "merge_min_speech_duration": mspd,
                    "merge_strategy": strat,
                }
            )
    return generated


def select_candidates(all_candidates: list[dict], max_candidates: int) -> list[dict]:
    if max_candidates <= 0 or len(all_candidates) <= max_candidates:
        return all_candidates
    if max_candidates == 1:
        return [all_candidates[0]]

    picks = []
    last_idx = len(all_candidates) - 1
    for i in range(max_candidates):
        idx = round(i * last_idx / (max_candidates - 1))
        picks.append(all_candidates[idx])

    unique = []
    seen = set()
    for candidate in picks:
        name = candidate["name"]
        if name in seen:
            continue
        seen.add(name)
        unique.append(candidate)
    return unique


def run_validate(entry: dict, config_path: Path, report_path: Path) -> dict:
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--",
        "validate",
        entry["input_media"],
        f"--{entry['truth_type'].replace('_', '-')}",
        entry["truth_path"],
        "--profile",
        entry["profile"],
        "--output",
        str(report_path),
        "--config",
        str(config_path),
    ]
    total_ms = entry.get("total_ms")
    if total_ms is not None:
        cmd.extend(["--total-ms", str(int(total_ms))])
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(
            f"validation failed for {entry['id']}\n"
            f"command: {' '.join(cmd)}\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )
    return json.loads(report_path.read_text(encoding="utf-8"))


def apply_candidate(base: dict, candidate: dict) -> dict:
    updated = dict(base)
    updated["vad_engine"] = "spectral"
    for key in [
        "energy_threshold",
        "min_non_voice_ms",
        "min_speech_ms",
        "spectral_entropy_min",
        "spectral_flatness_max",
        "spectral_entropy_max",
        "spectral_centroid_min",
        "spectral_centroid_max",
    ]:
        if key in candidate:
            updated[key] = candidate[key]

    updated["merge_gap_ms"] = candidate["merge_gap_ms"]
    merge_options = dict(updated.get("merge_options") or {})
    merge_options["min_gap_to_merge"] = candidate["merge_min_gap_to_merge"]
    merge_options["min_silence_duration"] = candidate["merge_min_silence_duration"]
    merge_options["min_speech_duration"] = candidate["merge_min_speech_duration"]
    merge_options["merge_strategy"] = candidate["merge_strategy"]
    if "silence_threshold_db" not in merge_options:
        merge_options["silence_threshold_db"] = -42
    updated["merge_options"] = merge_options
    return updated


def evaluate(
    manifest: dict,
    holdout_id: str,
    guard_ids: set[str],
    candidate: dict,
    base_legacy_config: dict,
    base_modern_config: dict,
    baseline: dict[str, dict],
    apply_to_modern: bool,
    max_modern_drop: float,
    precision_floor: float,
    objective: str,
    tmp_dir: Path,
) -> dict:
    entries = {entry["id"]: entry for entry in manifest["entries"]}
    candidate_config = apply_candidate(base_legacy_config, candidate)
    candidate_path = tmp_dir / f"{candidate['name']}.json"
    candidate_path.write_text(
        json.dumps(candidate_config, indent=2) + "\n", encoding="utf-8"
    )

    holdout_report = run_validate(
        entries[holdout_id],
        candidate_path,
        tmp_dir / f"{candidate['name']}-{holdout_id}.json",
    )
    holdout_precision = metric_value(holdout_report, "non_voice_precision")
    holdout_recall = metric_value(holdout_report, "non_voice_recall")
    holdout_overlap = float(holdout_report.get("overlap_ratio") or 0.0)
    recall_overlap_h = harmonic_mean(holdout_recall, holdout_overlap)
    worst_of_three = min(holdout_precision, holdout_recall, holdout_overlap)
    h3 = harmonic_mean3(holdout_precision, holdout_recall, holdout_overlap)
    holdout_score = 0.8 * recall_overlap_h + 0.2 * holdout_precision
    if objective == "worst3":
        holdout_score = worst_of_three
    elif objective == "h3":
        holdout_score = h3
    if holdout_precision < precision_floor:
        holdout_score *= holdout_precision / max(precision_floor, 1e-9)

    guard_reports = []
    modern_pass = True
    modern_failures = []
    for guard_id in sorted(guard_ids):
        guard_entry = entries[guard_id]
        guard_config_path = candidate_path
        if not apply_to_modern:
            guard_config_path = Path(guard_entry["config_path"])
        elif base_modern_config:
            merged = apply_candidate(base_modern_config, candidate)
            guard_config_path = tmp_dir / f"{candidate['name']}-{guard_id}-modern.json"
            guard_config_path.write_text(
                json.dumps(merged, indent=2) + "\n", encoding="utf-8"
            )

        report = run_validate(
            guard_entry,
            guard_config_path,
            tmp_dir / f"{candidate['name']}-{guard_id}.json",
        )
        precision = metric_value(report, "non_voice_precision")
        recall = metric_value(report, "non_voice_recall")
        overlap = float(report.get("overlap_ratio") or 0.0)
        guard_reports.append(
            {
                "id": guard_id,
                "non_voice_precision": precision,
                "non_voice_recall": recall,
                "overlap_ratio": overlap,
            }
        )
        baseline_row = baseline[guard_id]
        floor_recall = max(0.0, baseline_row["non_voice_recall"] - max_modern_drop)
        floor_overlap = max(0.0, baseline_row["overlap_ratio"] - max_modern_drop)
        if recall < floor_recall:
            modern_pass = False
            modern_failures.append(
                f"{guard_id}: recall={recall:.4f} < floor={floor_recall:.4f}"
            )
        if overlap < floor_overlap:
            modern_pass = False
            modern_failures.append(
                f"{guard_id}: overlap={overlap:.4f} < floor={floor_overlap:.4f}"
            )

    return {
        "candidate": candidate,
        "holdout": {
            "id": holdout_id,
            "non_voice_precision": holdout_precision,
            "non_voice_recall": holdout_recall,
            "overlap_ratio": holdout_overlap,
            "worst_of_three": worst_of_three,
            "h3": h3,
            "score": holdout_score,
        },
        "modern_guard_pass": modern_pass,
        "modern_guard_failures": modern_failures,
        "modern_checks": guard_reports,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Search radio-play holdout candidates with modern guardrails"
    )
    parser.add_argument(
        "--manifest",
        default="testdata/validation/radio-play-manifest.json",
        help="Validation manifest",
    )
    parser.add_argument(
        "--baseline-summary",
        default="analysis/validation/radio-play-sweep-summary.json",
        help="Baseline sweep summary for modern guard floors",
    )
    parser.add_argument(
        "--holdout-id",
        default="the_hole_1962_radio",
        help="Holdout entry id",
    )
    parser.add_argument(
        "--modern-guard-id",
        action="append",
        default=["elephants_dream_2006_de_radio", "elephants_dream_2006_es_radio"],
        help="Modern guard entry id (repeatable)",
    )
    parser.add_argument(
        "--legacy-config",
        default="config/profiles/legacy-optimized.json",
        help="Base legacy config",
    )
    parser.add_argument(
        "--modern-config",
        default="config/profiles/modern-optimized.json",
        help="Base modern config",
    )
    parser.add_argument(
        "--max-modern-drop",
        type=float,
        default=0.02,
        help="Maximum allowed absolute drop on modern guard recall/overlap",
    )
    parser.add_argument(
        "--apply-to-modern",
        action="store_true",
        help="Apply candidate tuning to modern guard entries too",
    )
    parser.add_argument(
        "--max-candidates",
        type=int,
        default=16,
        help="Maximum candidate count to evaluate",
    )
    parser.add_argument(
        "--search-mode",
        choices=["basic", "extended"],
        default="basic",
        help="basic tunes threshold+merge; extended also tunes entropy max and centroid bounds",
    )
    parser.add_argument(
        "--precision-floor",
        type=float,
        default=0.25,
        help="Soft floor for holdout precision; below floor score is down-weighted",
    )
    parser.add_argument(
        "--objective",
        choices=["weighted", "worst3", "h3"],
        default="weighted",
        help="Holdout ranking objective: weighted, worst3(min metric), or h3(harmonic mean of P/R/O)",
    )
    parser.add_argument(
        "--output",
        default="analysis/optimization/radio-play-holdout-search.json",
        help="Output report path",
    )
    args = parser.parse_args()

    manifest_path = Path(args.manifest)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    guard_ids = set(args.modern_guard_id)
    baseline = baseline_metrics(Path(args.baseline_summary), guard_ids)

    legacy_config = json.loads(Path(args.legacy_config).read_text(encoding="utf-8"))
    modern_config = json.loads(Path(args.modern_config).read_text(encoding="utf-8"))

    started = time.time()
    rows = []
    with tempfile.TemporaryDirectory(prefix="radio-play-holdout-") as temp_dir:
        tmp_dir = Path(temp_dir)
        selected = select_candidates(
            candidates(args.search_mode), max(args.max_candidates, 1)
        )
        for candidate in selected:
            rows.append(
                evaluate(
                    manifest=manifest,
                    holdout_id=args.holdout_id,
                    guard_ids=guard_ids,
                    candidate=candidate,
                    base_legacy_config=legacy_config,
                    base_modern_config=modern_config,
                    baseline=baseline,
                    apply_to_modern=args.apply_to_modern,
                    max_modern_drop=args.max_modern_drop,
                    precision_floor=args.precision_floor,
                    objective=args.objective,
                    tmp_dir=tmp_dir,
                )
            )

    ranked = sorted(
        rows,
        key=lambda row: (
            0 if row["modern_guard_pass"] else 1,
            -row["holdout"]["score"],
            -row["holdout"]["non_voice_recall"],
            -row["holdout"]["non_voice_precision"],
            -row["holdout"]["overlap_ratio"],
        ),
    )

    report = {
        "manifest": str(manifest_path),
        "baseline_summary": args.baseline_summary,
        "holdout_id": args.holdout_id,
        "modern_guard_ids": sorted(guard_ids),
        "apply_to_modern": bool(args.apply_to_modern),
        "max_modern_drop": args.max_modern_drop,
        "search_mode": args.search_mode,
        "precision_floor": args.precision_floor,
        "objective": args.objective,
        "candidate_count": len(rows),
        "elapsed_ms": int((time.time() - started) * 1000),
        "ranked_candidates": ranked,
        "best_candidate": ranked[0] if ranked else None,
    }

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"wrote holdout optimization report: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
