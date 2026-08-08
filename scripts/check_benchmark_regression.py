#!/usr/bin/env python3
import argparse
import json
import sys
from pathlib import Path

REQUIRED_STAGE_FIELDS = [
    "decode_ms",
    "resample_ms",
    "frame_ms",
    "vad_ms",
    "smooth_ms",
    "speech_ms",
    "merge_ms",
    "invert_ms",
]
ABSOLUTE_TOLERANCE_MS = 2000
RELATIVE_TOLERANCE = 0.50


def load_json(path: Path) -> dict:
    try:
        with path.open("r", encoding="utf-8") as handle:
            return json.load(handle)
    except FileNotFoundError:
        print(f"benchmark baseline missing: {path}", file=sys.stderr)
        raise


def require_int(data: dict, field: str) -> int:
    value = data.get(field)
    if not isinstance(value, int):
        raise ValueError(f"missing or invalid integer field: {field}")
    return value


def require_str(data: dict, field: str) -> str:
    value = data.get(field)
    if not isinstance(value, str):
        raise ValueError(f"missing or invalid string field: {field}")
    return value


def require_stage_map(data: dict) -> dict:
    stage_ms = data.get("stage_ms")
    if not isinstance(stage_ms, dict):
        raise ValueError("missing or invalid object field: stage_ms")
    for field in REQUIRED_STAGE_FIELDS:
        value = stage_ms.get(field)
        if not isinstance(value, int):
            raise ValueError(f"missing or invalid integer field: stage_ms.{field}")
    return stage_ms


def allowed_regression_ms(baseline_ms: int) -> int:
    return max(ABSOLUTE_TOLERANCE_MS, int(baseline_ms * RELATIVE_TOLERANCE))


def compare_exact(name: str, baseline, candidate, failures: list[str]) -> None:
    if baseline != candidate:
        failures.append(f"{name} changed: baseline={baseline!r} candidate={candidate!r}")


def compare_timing(name: str, baseline_ms: int, candidate_ms: int, failures: list[str]) -> None:
    delta_ms = candidate_ms - baseline_ms
    tolerance_ms = allowed_regression_ms(baseline_ms)
    if delta_ms > tolerance_ms:
        failures.append(
            f"{name} regressed by {delta_ms}ms "
            f"(baseline={baseline_ms}ms candidate={candidate_ms}ms tolerance={tolerance_ms}ms)"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description="Check benchmark regression against baseline JSON")
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--candidate", required=True)
    args = parser.parse_args()

    baseline_path = Path(args.baseline)
    candidate_path = Path(args.candidate)

    if not baseline_path.exists():
        print(f"No benchmark baseline found at {baseline_path}; skipping regression check")
        return 0

    baseline = load_json(baseline_path)
    candidate = load_json(candidate_path)

    baseline_input = require_str(baseline, "input_file")
    candidate_input = require_str(candidate, "input_file")
    baseline_frame_count = require_int(baseline, "frame_count")
    candidate_frame_count = require_int(candidate, "frame_count")
    baseline_segment_count = require_int(baseline, "segment_count")
    candidate_segment_count = require_int(candidate, "segment_count")
    baseline_total_ms = require_int(baseline, "total_ms")
    candidate_total_ms = require_int(candidate, "total_ms")
    baseline_decode_ms = require_int(baseline, "decode_ms")
    candidate_decode_ms = require_int(candidate, "decode_ms")
    baseline_stage = require_stage_map(baseline)
    candidate_stage = require_stage_map(candidate)

    if baseline_decode_ms != baseline_stage["decode_ms"]:
        raise ValueError("baseline artifact invalid: decode_ms does not match stage_ms.decode_ms")
    if candidate_decode_ms != candidate_stage["decode_ms"]:
        raise ValueError("candidate artifact invalid: decode_ms does not match stage_ms.decode_ms")

    failures: list[str] = []
    compare_exact("input_file", baseline_input, candidate_input, failures)
    compare_exact("frame_count", baseline_frame_count, candidate_frame_count, failures)
    compare_exact("segment_count", baseline_segment_count, candidate_segment_count, failures)
    compare_timing("total_ms", baseline_total_ms, candidate_total_ms, failures)

    for field in REQUIRED_STAGE_FIELDS:
        compare_timing(
            f"stage_ms.{field}",
            baseline_stage[field],
            candidate_stage[field],
            failures,
        )

    print("Benchmark comparison summary:")
    print(f"  input_file: {candidate_input}")
    print(f"  frame_count: {candidate_frame_count}")
    print(f"  segment_count: {candidate_segment_count}")
    print(f"  total_ms: baseline={baseline_total_ms} candidate={candidate_total_ms}")

    if failures:
        print("Benchmark regression check failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print("Benchmark regression check passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as err:
        print(f"benchmark artifact error: {err}", file=sys.stderr)
        raise SystemExit(1)
