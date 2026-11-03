#!/usr/bin/env python3
import argparse
import glob
import json
import sys
from pathlib import Path

VALID_TRUTH_TYPES = {"truth_json", "subtitles", "dataset_manifest"}
VALID_TRUTH_SOURCES = {
    "synthetic",
    "authored_subtitles",
    "embedded_subtitles",
    "whisper_derived",
}
VALID_LICENSE_STATUSES = {
    "public_domain",
    "cc_by",
    "cc_by_sa",
    "cc0",
    "other_permissive",
    "synthetic",
}
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


def validate_provenance(entry: dict, entry_id: str, failures: list[str]) -> None:
    provenance = entry.get("provenance")
    if not isinstance(provenance, dict):
        failures.append(f"{entry_id}: missing provenance object")
        return
    for field in ("source_url", "license", "license_status", "retrieved"):
        value = provenance.get(field)
        if not isinstance(value, str) or not value.strip():
            failures.append(f"{entry_id}: provenance missing '{field}'")
    status = provenance.get("license_status")
    if isinstance(status, str) and status not in VALID_LICENSE_STATUSES:
        failures.append(
            f"{entry_id}: provenance license_status '{status}' not in allowed set"
        )

    truth_source = entry.get("truth_source")
    if truth_source not in VALID_TRUTH_SOURCES:
        failures.append(
            f"{entry_id}: truth_source must be one of {sorted(VALID_TRUTH_SOURCES)}"
        )
    language = entry.get("language")
    if not isinstance(language, str) or not language.strip():
        failures.append(f"{entry_id}: missing language")

    expected = entry.get("expected_output")
    if not isinstance(expected, dict):
        failures.append(f"{entry_id}: missing expected_output object")
        return
    leak = expected.get("max_dialogue_leak_ratio")
    if not isinstance(leak, (int, float)) or not 0 <= leak <= 1:
        failures.append(
            f"{entry_id}: expected_output.max_dialogue_leak_ratio must be 0..1"
        )
    retained = expected.get("min_narratable_retained_ms")
    if not isinstance(retained, int) or retained < 0:
        failures.append(
            f"{entry_id}: expected_output.min_narratable_retained_ms must be >= 0"
        )


def check_cohorts(
    entries: list[dict], min_entries: int, failures: list[str]
) -> None:
    counts: dict[str, int] = {}
    for entry in entries:
        cohort = entry.get("cohort")
        if not isinstance(cohort, str) or not cohort.strip():
            failures.append(f"{entry.get('id', 'entry')}: missing cohort")
            continue
        counts[cohort] = counts.get(cohort, 0) + 1
    for cohort in ("modern", "legacy"):
        if counts.get(cohort, 0) < min_entries:
            failures.append(
                f"cohort '{cohort}' has {counts.get(cohort, 0)} entries, "
                f"need at least {min_entries}"
            )


def check_languages(entries: list[dict], minimum: int, failures: list[str]) -> None:
    languages = {
        entry.get("language")
        for entry in entries
        if isinstance(entry.get("language"), str) and entry.get("language") != "und"
    }
    if len(languages) < minimum:
        failures.append(
            f"only {len(languages)} spoken languages represented "
            f"({', '.join(sorted(languages)) or 'none'}), need at least {minimum}"
        )


def check_holdout_separation(
    entries: list[dict], tuning_glob: str, failures: list[str]
) -> None:
    holdout_ids = {
        entry.get("id")
        for entry in entries
        if str(entry.get("tier", "")).upper() == "C"
    }
    for path in sorted(glob.glob(tuning_glob)):
        try:
            payload = json.loads(Path(path).read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        tuned = payload.get("holdout_id")
        if isinstance(tuned, str) and tuned in holdout_ids:
            failures.append(
                f"{tuned}: holdout entry is used as a tuning target in {path}"
            )


def check_readiness_report(path: Path, holdout_ids: set[str], failures: list[str]) -> None:
    if not path.exists():
        failures.append(f"readiness report missing: {path}")
        return
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as err:
        failures.append(f"readiness report is not valid JSON ({path}): {err}")
        return
    for section in ("detector_only", "cue_assisted"):
        if section not in report:
            failures.append(f"readiness report missing '{section}' section")
    reported = {
        row.get("id")
        for row in report.get("detector_only", {}).get("entries", [])
        if isinstance(row, dict)
    }
    missing = sorted(holdout_ids - reported)
    if missing:
        failures.append(
            "readiness report omits holdout entries: " + ", ".join(missing)
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
    parser.add_argument(
        "--require-provenance",
        action="store_true",
        help="require provenance, truth_source, language and expected_output metadata",
    )
    parser.add_argument(
        "--require-cohorts",
        action="store_true",
        help="require modern and legacy cohorts to each meet --min-cohort-entries",
    )
    parser.add_argument(
        "--min-cohort-entries",
        type=int,
        default=2,
        help="minimum entries per cohort when --require-cohorts is set",
    )
    parser.add_argument(
        "--min-languages",
        type=int,
        default=0,
        help="minimum distinct spoken languages across selected entries",
    )
    parser.add_argument(
        "--require-holdout-separation",
        action="store_true",
        help="fail when a tier C holdout is used as a tuning target",
    )
    parser.add_argument(
        "--tuning-artifacts",
        default="analysis/optimization/*.json",
        help="glob of tuning artifacts scanned for holdout ids",
    )
    parser.add_argument(
        "--readiness-report",
        help="readiness report JSON that must separate detector-only and cue-assisted results",
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
    selected_entries: list[dict] = []
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
        selected_entries.append(entry)
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

        if args.require_provenance:
            validate_provenance(entry, entry_id, failures)

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

    if args.require_cohorts:
        check_cohorts(selected_entries, args.min_cohort_entries, failures)
    if args.min_languages > 0:
        check_languages(selected_entries, args.min_languages, failures)
    if args.require_holdout_separation:
        check_holdout_separation(selected_entries, args.tuning_artifacts, failures)
    if args.readiness_report:
        holdout_ids = {
            entry.get("id")
            for entry in selected_entries
            if str(entry.get("tier", "")).upper() == "C"
        }
        check_readiness_report(Path(args.readiness_report), holdout_ids, failures)

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
