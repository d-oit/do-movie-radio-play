#!/usr/bin/env python3
"""Tests for holdout reporting: cohorts, trend deltas and window diagnosis.

No media and no model: every case is a small in-memory fixture.
"""

import json
import sys
import tempfile
from pathlib import Path

from build_radio_play_readiness_report import cohort_summary, entry_metadata
from build_radio_play_trend import delta, point_from_summary
from check_validation_coverage import check_cohorts, check_holdout_separation, check_languages
from export_radio_play_window_diagnosis import build_examples, complement

FAILURES: list[str] = []


def check(label: str, got, want) -> None:
    if got != want:
        FAILURES.append(f"{label}: got {got!r}, want {want!r}")


def test_cohort_summary() -> None:
    rows = [
        {"id": "a", "overlap_ratio": 0.9, "non_voice_precision": 0.9,
         "non_voice_recall": 0.9, "overlap_ratio_lb95": 0.8,
         "non_voice_precision_lb95": 0.8, "non_voice_recall_lb95": 0.8,
         "language": "en", "failing": False},
        {"id": "b", "overlap_ratio": 0.5, "non_voice_precision": 0.5,
         "non_voice_recall": 0.5, "overlap_ratio_lb95": 0.4,
         "non_voice_precision_lb95": 0.4, "non_voice_recall_lb95": 0.4,
         "language": "fr", "failing": True},
    ]
    summary = cohort_summary(rows)
    check("cohort count", summary["count"], 2)
    check("cohort languages", summary["languages"], ["en", "fr"])
    check("worst entry", summary["worst_entry"], "b")
    check("failing entries", summary["failing_entries"], ["b"])
    check("empty cohort", cohort_summary([])["worst_entry"], None)


def test_entry_metadata() -> None:
    manifest = {"x": {"cohort": "legacy", "language": "fr", "vad_engine": "silero"}}
    meta = entry_metadata({"id": "x", "input_media": "testdata/raw/charade_1963.webm"}, manifest)
    check("manifest cohort wins", meta["cohort"], "legacy")
    check("manifest language", meta["language"], "fr")
    fallback = entry_metadata({"id": "y", "input_media": "testdata/raw/plan9_1959.webm"}, {})
    check("filename fallback", fallback["cohort"], "legacy")


def test_trend() -> None:
    summary = {
        "results": [
            {"id": "a", "cohort": "modern", "language": "en",
             "metrics": {"overlap_ratio": 0.9, "non_voice_precision": 0.8,
                         "non_voice_recall": 0.7}},
        ],
        "pending": [{"id": "p", "status": "pending_assets"}],
    }
    point = point_from_summary(summary, "2026-11-03", ["note"])
    check("trend entry count", point["entry_count"], 1)
    check("trend pending kept", point["pending"][0]["id"], "p")
    check("delta up", delta(0.9, 0.8), "+0.1000")
    check("delta missing", delta(None, 0.8), "-")


def test_window_diagnosis() -> None:
    check("complement", complement([(1000, 2000)], 4000), [(0, 1000), (2000, 4000)])
    predicted_non_voice = [
        {"start_ms": 0, "end_ms": 1000, "kind": "non_voice", "tags": ["music"]},
        {"start_ms": 2000, "end_ms": 4000, "kind": "non_voice", "tags": []},
    ]
    predicted_speech = [{"start_ms": 1000, "end_ms": 2000, "kind": "speech", "tags": []}]
    truth_speech = [(500, 1500)]
    examples = build_examples(
        predicted_non_voice, predicted_speech, truth_speech, [], 4000, 100
    )
    kinds = {example["kind"] for example in examples}
    check("fp detected", "false_positive" in kinds, True)
    check("fn detected", "false_negative" in kinds, True)
    fp = next(e for e in examples if e["kind"] == "false_positive")
    check("fp tags", fp["predicted_tags"], ["music"])
    check("fp truth label", fp["truth_label"], "speech")


def test_coverage_checks() -> None:
    failures: list[str] = []
    check_cohorts(
        [{"id": "a", "cohort": "modern"}, {"id": "b", "cohort": "legacy"}], 2, failures
    )
    check("cohort breadth failure", len(failures), 2)

    failures = []
    check_languages([{"language": "en"}, {"language": "de"}], 2, failures)
    check("two languages pass", failures, [])

    with tempfile.TemporaryDirectory() as tmp:
        tuning = Path(tmp) / "tuning.json"
        tuning.write_text(json.dumps({"holdout_id": "holdout"}), encoding="utf-8")
        failures = []
        check_holdout_separation(
            [{"id": "holdout", "tier": "C"}], str(Path(tmp) / "*.json"), failures
        )
        check("holdout tuning failure", len(failures), 1)


def main() -> int:
    test_cohort_summary()
    test_entry_metadata()
    test_trend()
    test_window_diagnosis()
    test_coverage_checks()
    if FAILURES:
        print("radio-play reporting tests failed:", file=sys.stderr)
        for failure in FAILURES:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print("radio-play reporting tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
