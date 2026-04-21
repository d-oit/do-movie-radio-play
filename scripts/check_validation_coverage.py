#!/usr/bin/env python3
import argparse
import json
import sys
from pathlib import Path

VALID_TRUTH_TYPES = {"truth_json", "subtitles", "dataset_manifest"}
REQUIRED_REPORT_FIELDS = [
    "profile",
    "tolerance_ms",
    "expected_segments",
    "predicted_segments",
    "overlap_ratio",
    "boundary_error_ms",
    "speech_precision",
    "speech_recall",
    "non_voice_precision",
    "non_voice_recall",
    "speech_time_precision",
    "speech_time_recall",
    "non_voice_time_precision",
    "non_voice_time_recall",
    "speech_overlap_ms",
    "speech_predicted_ms",
    "speech_expected_ms",
    "non_voice_overlap_ms",
    "non_voice_predicted_ms",
    "non_voice_expected_ms",
]


def load_manifest(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def require_string(entry: dict, field: str, failures: list[str], entry_id: str) -> str:
    value = entry.get(field)
    if isinstance(value, str) and value.strip():
        return value
    failures.append(f"{entry_id}: missing or invalid string field '{field}'")
    return ""


def validate_report(
    entry_id: str, report_path: Path, expected_profile: str, failures: list[str]
) -> None:
    if not report_path.exists():
        failures.append(f"{entry_id}: output report missing: {report_path}")
        return

    try:
        with report_path.open("r", encoding="utf-8") as handle:
            report = json.load(handle)
    except json.JSONDecodeError as err:
        failures.append(
            f"{entry_id}: output report is not valid JSON ({report_path}): {err}"
        )
        return

    for field in REQUIRED_REPORT_FIELDS:
        if field not in report:
            failures.append(
                f"{entry_id}: report missing field '{field}' ({report_path})"
            )

    if expected_profile and report.get("profile") != expected_profile:
        failures.append(
            f"{entry_id}: report profile mismatch (expected={expected_profile!r}, got={report.get('profile')!r})"
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate production-eval manifest coverage and report artifacts"
    )
    parser.add_argument(
        "--manifest",
        default="testdata/validation/manifest.json",
        help="path to production eval manifest",
    )
    parser.add_argument(
        "--tier",
        action="append",
        default=["A"],
        help="tier(s) to enforce (repeatable). Default: A",
    )
    parser.add_argument(
        "--strict-files",
        action="store_true",
        help="fail if input/truth files are missing for selected entries",
    )
    args = parser.parse_args()

    manifest_path = Path(args.manifest)
    if not manifest_path.exists():
        print(f"manifest missing: {manifest_path}", file=sys.stderr)
        return 1

    try:
        manifest = load_manifest(manifest_path)
    except json.JSONDecodeError as err:
        print(f"manifest is not valid JSON ({manifest_path}): {err}", file=sys.stderr)
        return 1

    entries = manifest.get("entries")
    if not isinstance(entries, list) or not entries:
        print("manifest entries must be a non-empty list", file=sys.stderr)
        return 1

    selected_tiers = {tier.upper() for tier in args.tier}
    failures: list[str] = []
    seen_ids: set[str] = set()
    seen_outputs: set[str] = set()
    checked = 0

    for idx, entry in enumerate(entries, start=1):
        if not isinstance(entry, dict):
            failures.append(f"entry[{idx}] is not an object")
            continue

        entry_id = (
            entry.get("id") if isinstance(entry.get("id"), str) else f"entry[{idx}]"
        )
        tier = str(entry.get("tier", "")).upper()
        if tier not in selected_tiers:
            continue

        checked += 1
        if entry_id in seen_ids:
            failures.append(f"{entry_id}: duplicate id")
        seen_ids.add(entry_id)

        input_media = require_string(entry, "input_media", failures, entry_id)
        truth_type = require_string(entry, "truth_type", failures, entry_id)
        truth_path = require_string(entry, "truth_path", failures, entry_id)
        output_report = require_string(entry, "output_report", failures, entry_id)
        profile = require_string(entry, "profile", failures, entry_id)
        config_path = entry.get("config_path")
        if config_path is not None and not (
            isinstance(config_path, str) and config_path.strip()
        ):
            failures.append(
                f"{entry_id}: config_path must be a non-empty string when set"
            )
            config_path = None

        if truth_type and truth_type not in VALID_TRUTH_TYPES:
            failures.append(f"{entry_id}: invalid truth_type '{truth_type}'")

        if truth_type == "subtitles":
            total_ms = entry.get("total_ms")
            if not isinstance(total_ms, int) or total_ms <= 0:
                failures.append(
                    f"{entry_id}: subtitles entries require positive integer total_ms"
                )

        if output_report:
            if output_report in seen_outputs:
                failures.append(
                    f"{entry_id}: duplicate output_report '{output_report}'"
                )
            seen_outputs.add(output_report)

        if args.strict_files:
            if input_media and not Path(input_media).exists():
                failures.append(f"{entry_id}: input_media missing: {input_media}")
            if truth_path and not Path(truth_path).exists():
                failures.append(f"{entry_id}: truth_path missing: {truth_path}")
            if config_path and not Path(config_path).exists():
                failures.append(f"{entry_id}: config_path missing: {config_path}")

        if output_report:
            validate_report(entry_id, Path(output_report), profile, failures)

    if checked == 0:
        print(
            f"no manifest entries matched selected tiers ({', '.join(sorted(selected_tiers))})",
            file=sys.stderr,
        )
        return 1

    print(f"checked manifest entries: {checked}")
    print(f"selected tiers: {', '.join(sorted(selected_tiers))}")

    if failures:
        print("validation coverage check failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print("validation coverage check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
