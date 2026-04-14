#!/usr/bin/env python3
import argparse
import json
import subprocess
import time
from pathlib import Path

VALID_TRUTH_TYPES = {"truth_json", "subtitles", "dataset_manifest"}


def run_entry(entry: dict) -> dict:
    entry_id = entry["id"]
    truth_type = entry["truth_type"]
    if truth_type not in VALID_TRUTH_TYPES:
        raise ValueError(f"{entry_id}: invalid truth_type '{truth_type}'")

    input_media = Path(entry["input_media"])
    truth_path = Path(entry["truth_path"])
    output_report = Path(entry["output_report"])
    profile = entry["profile"]

    if not input_media.exists():
        raise FileNotFoundError(f"{entry_id}: missing input_media {input_media}")
    if not truth_path.exists():
        raise FileNotFoundError(f"{entry_id}: missing truth_path {truth_path}")

    output_report.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--",
        "validate",
        str(input_media),
        f"--{truth_type.replace('_', '-')}",
        str(truth_path),
        "--profile",
        profile,
        "--output",
        str(output_report),
    ]

    total_ms = entry.get("total_ms")
    if truth_type in {"subtitles", "dataset_manifest"}:
        if not isinstance(total_ms, int) or total_ms <= 0:
            raise ValueError(
                f"{entry_id}: positive integer total_ms required for truth_type {truth_type}"
            )
        cmd.extend(["--total-ms", str(total_ms)])

    started = time.time()
    result = subprocess.run(cmd, capture_output=True, text=True)
    elapsed_ms = int((time.time() - started) * 1000)

    if result.returncode != 0:
        raise RuntimeError(
            f"{entry_id}: validation command failed (exit={result.returncode})\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )

    with output_report.open("r", encoding="utf-8") as handle:
        report = json.load(handle)

    return {
        "id": entry_id,
        "tier": entry["tier"],
        "input_media": str(input_media),
        "truth_type": truth_type,
        "truth_path": str(truth_path),
        "profile": profile,
        "output_report": str(output_report),
        "elapsed_ms": elapsed_ms,
        "metrics": {
            "overlap_ratio": report.get("overlap_ratio"),
            "boundary_error_ms": report.get("boundary_error_ms"),
            "speech_precision": report.get("speech_precision"),
            "speech_recall": report.get("speech_recall"),
            "non_voice_precision": report.get("non_voice_precision"),
            "non_voice_recall": report.get("non_voice_recall"),
            "expected_segments": report.get("expected_segments"),
            "predicted_segments": report.get("predicted_segments"),
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run validation manifest entries and write sweep summary"
    )
    parser.add_argument("--manifest", default="testdata/validation/manifest.json")
    parser.add_argument(
        "--summary", default="analysis/validation/full-sweep-summary.json"
    )
    parser.add_argument("--tier", action="append", default=["A", "B", "C"])
    args = parser.parse_args()

    manifest_path = Path(args.manifest)
    with manifest_path.open("r", encoding="utf-8") as handle:
        manifest = json.load(handle)

    entries = manifest.get("entries")
    if not isinstance(entries, list) or not entries:
        raise ValueError("manifest entries must be a non-empty list")

    selected = {tier.upper() for tier in args.tier}
    selected_entries = [
        entry for entry in entries if str(entry.get("tier", "")).upper() in selected
    ]
    if not selected_entries:
        raise ValueError(f"no entries matched tiers: {', '.join(sorted(selected))}")

    results = []
    started = time.time()
    for entry in selected_entries:
        results.append(run_entry(entry))

    summary = {
        "manifest": str(manifest_path),
        "selected_tiers": sorted(selected),
        "entry_count": len(results),
        "elapsed_ms": int((time.time() - started) * 1000),
        "results": results,
    }

    summary_path = Path(args.summary)
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    with summary_path.open("w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)
        handle.write("\n")

    print(f"validation sweep complete: {len(results)} entries")
    print(f"summary: {summary_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
