#!/usr/bin/env python3
import argparse
import json
import sys
from pathlib import Path

ALLOWED_LICENSE_STATUSES = {
    "public_domain",
    "cc_by",
    "cc_by_sa",
    "cc0",
    "other_permissive",
}


def require_non_empty_string(
    obj: dict, key: str, failures: list[str], prefix: str
) -> str:
    value = obj.get(key)
    if isinstance(value, str) and value.strip():
        return value.strip()
    failures.append(f"{prefix}: missing or invalid '{key}'")
    return ""


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate candidate media intake list for legal + subtitle readiness"
    )
    parser.add_argument(
        "--input",
        default="analysis/media/modern-multilang-intake.json",
        help="Path to intake candidate JSON",
    )
    parser.add_argument(
        "--min-sub-langs",
        type=int,
        default=2,
        help="Minimum subtitle language count required per candidate",
    )
    parser.add_argument(
        "--strict-license",
        action="store_true",
        help="Fail unless license_status is explicitly in the allowed list",
    )
    args = parser.parse_args()

    input_path = Path(args.input)
    if not input_path.exists():
        print(f"missing intake file: {input_path}", file=sys.stderr)
        return 1

    try:
        payload = json.loads(input_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as err:
        print(f"invalid json: {input_path}: {err}", file=sys.stderr)
        return 1

    candidates = payload.get("candidates")
    if not isinstance(candidates, list) or not candidates:
        print("intake must contain non-empty 'candidates' list", file=sys.stderr)
        return 1

    failures: list[str] = []
    for idx, candidate in enumerate(candidates, start=1):
        if not isinstance(candidate, dict):
            failures.append(f"entry[{idx}]: candidate must be an object")
            continue

        title = require_non_empty_string(candidate, "title", failures, f"entry[{idx}]")
        prefix = title if title else f"entry[{idx}]"
        require_non_empty_string(candidate, "source_url", failures, prefix)
        require_non_empty_string(candidate, "catalog_url", failures, prefix)

        subtitle_languages = candidate.get("subtitle_languages")
        if not isinstance(subtitle_languages, list) or any(
            not isinstance(item, str) or not item.strip() for item in subtitle_languages
        ):
            failures.append(f"{prefix}: subtitle_languages must be a string list")
            subtitle_languages = []

        if len(subtitle_languages) < args.min_sub_langs:
            failures.append(
                f"{prefix}: subtitle language count {len(subtitle_languages)} < {args.min_sub_langs}"
            )

        license_status = candidate.get("license_status")
        if not isinstance(license_status, str) or not license_status.strip():
            failures.append(f"{prefix}: missing license_status")
        elif args.strict_license and license_status not in ALLOWED_LICENSE_STATUSES:
            failures.append(
                f"{prefix}: license_status '{license_status}' not in allowed set"
            )

        rights_notes = candidate.get("rights_notes")
        if not isinstance(rights_notes, str) or not rights_notes.strip():
            failures.append(f"{prefix}: missing rights_notes")

    print(f"checked candidates: {len(candidates)}")
    if failures:
        print("media intake validation failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print("media intake validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
