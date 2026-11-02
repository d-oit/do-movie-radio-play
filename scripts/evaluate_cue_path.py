#!/usr/bin/env python3
"""Measure the shipped dialogue-cue path end to end.

The pipeline vetoes narration inside dialogue cues, so the quality question is
not detector accuracy but how much true dialogue survives as narratable time.
This tool scores truth subtitles against cue files (exact, embedded, or derived
Whisper) and reports dialogue-time recall, false cue coverage, retained
narratable time, narration-on-dialogue leakage, missed short utterances, and
Wilson intervals, per film, cue source, and language.

Subcommands:
  film      score one film
  fixtures  run the deterministic regression fixtures (PR gate)
  sweep     evaluate pad/threshold configs on a dev split and report holdout
"""

import argparse
import json
import math
import re
import subprocess
import sys
import tempfile
from pathlib import Path

TIMESTAMP = re.compile(
    r"(\d\d):(\d\d):(\d\d)[,.](\d+)\s*-->\s*(\d\d):(\d\d):(\d\d)[,.](\d+)"
)

# Keep in sync with scripts/transcribe_cues.py.
DEFAULT_MODEL_REVISION = "ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66"


def parse_srt_entries(path: Path) -> list[tuple[float, float, str]]:
    text = path.read_text(encoding="utf-8-sig", errors="ignore")
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    entries = []
    for match in TIMESTAMP.finditer(text):
        h1, m1, s1, f1, h2, m2, s2, f2 = match.groups()
        start = int(h1) * 3600 + int(m1) * 60 + int(s1) + int(f1) / 1000
        end = int(h2) * 3600 + int(m2) * 60 + int(s2) + int(f2) / 1000
        if end > start:
            tail = text[match.end() :]
            body = tail.split("\n\n", 1)[0].strip()
            entries.append((start, end, body))
    return entries


def parse_srt(path: Path) -> list[tuple[float, float]]:
    return [(start, end) for start, end, _ in parse_srt_entries(path)]


def is_shouted(text: str) -> bool:
    letters = [char for char in text if char.isalpha()]
    return "!" in text or "[shout" in text.lower() or (
        bool(letters) and all(char.isupper() for char in letters)
    )


def merge(spans: list[tuple[float, float]]) -> list[tuple[float, float]]:
    out: list[tuple[float, float]] = []
    for start, end in sorted(spans):
        if end <= start:
            continue
        if out and start <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], end))
        else:
            out.append((start, end))
    return out


def total(spans: list[tuple[float, float]]) -> float:
    return sum(end - start for start, end in spans)


def intersect(
    left: list[tuple[float, float]], right: list[tuple[float, float]]
) -> list[tuple[float, float]]:
    left, right = merge(left), merge(right)
    out: list[tuple[float, float]] = []
    i = j = 0
    while i < len(left) and j < len(right):
        lo = max(left[i][0], right[j][0])
        hi = min(left[i][1], right[j][1])
        if hi > lo:
            out.append((lo, hi))
        if left[i][1] < right[j][1]:
            i += 1
        else:
            j += 1
    return out


def pad_spans(
    spans: list[tuple[float, float]], pad_s: float
) -> list[tuple[float, float]]:
    return [(max(0.0, start - pad_s), end + pad_s) for start, end in spans]


def subtract(
    base: list[tuple[float, float]], remove: list[tuple[float, float]]
) -> list[tuple[float, float]]:
    """Remaining parts of `base` once `remove` is cut out of it."""
    base, remove = merge(base), merge(remove)
    out: list[tuple[float, float]] = []
    for start, end in base:
        cursor = start
        for cut_start, cut_end in remove:
            if cut_end <= cursor:
                continue
            if cut_start >= end:
                break
            if cut_start > cursor:
                out.append((cursor, min(cut_start, end)))
            cursor = max(cursor, cut_end)
            if cursor >= end:
                break
        if cursor < end:
            out.append((cursor, end))
    return out


def load_detector_non_voice(path: Path) -> list[tuple[float, float]]:
    """Non-voice segments from a timeline JSON, in seconds."""
    return _load_segments(path, "non_voice")


def load_detector_speech(path: Path) -> list[tuple[float, float]]:
    """Speech segments from a timeline JSON, in seconds."""
    return _load_segments(path, "speech")


def _load_segments(path: Path, kind: str) -> list[tuple[float, float]]:
    payload = load_json(path)
    segments = payload.get("segments")
    if not isinstance(segments, list):
        raise ValueError(f"{path}: no 'segments' list")
    spans = []
    for seg in segments:
        if seg.get("kind") != kind:
            continue
        start = seg.get("start_ms")
        end = seg.get("end_ms")
        if isinstance(start, int) and isinstance(end, int) and end > start:
            spans.append((start / 1000, end / 1000))
    return merge(spans)


def wilson(k: int, n: int, z: float = 1.96) -> dict:
    if n <= 0:
        return {"p": 0.0, "lo": 0.0, "hi": 0.0, "n": 0}
    p = k / n
    denom = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return {"p": p, "lo": max(0.0, centre - half), "hi": min(1.0, centre + half), "n": n}


def covered_fraction(span: tuple[float, float], cues: list[tuple[float, float]]) -> float:
    duration = span[1] - span[0]
    if duration <= 0:
        return 0.0
    return total(intersect([span], cues)) / duration


def evaluate(
    truth: list[tuple[float, float]],
    cues: list[tuple[float, float]],
    total_s: float,
    pad_s: float,
    short_utterance_ms: int,
    truth_entries: list[tuple[float, float, str]] | None = None,
    detector_non_voice: list[tuple[float, float]] | None = None,
) -> dict:
    truth = merge(truth)
    cues = merge(pad_spans(cues, pad_s))
    truth_ms = total(truth) * 1000
    cue_ms = total(cues) * 1000
    covered_ms = total(intersect(truth, cues)) * 1000
    false_cue_ms = max(0.0, cue_ms - covered_ms)

    # The pipeline narrates only inside detector non-voice segments, minus cues.
    # Without a detector timeline the only honest denominator is the whole
    # non-dialogue timeline, which no shipped path can actually reach.
    if detector_non_voice is None:
        credited = [(0.0, max(0.0, total_s))]
        mode = "cue_only_upper_bound"
    else:
        credited = merge(detector_non_voice)
        mode = "detector_plus_cues"
    narration = subtract(credited, cues)
    narratable_ms = total(credited) * 1000
    narration_ms = total(narration) * 1000
    leak_ms = total(intersect(truth, narration)) * 1000
    retained_ms = max(0.0, narration_ms - leak_ms)
    leak_denom = narration_ms
    cue_veto_ms = max(0.0, narratable_ms - narration_ms)

    truth_segments = len(truth)
    covered_segments = sum(
        1 for span in truth if covered_fraction(span, cues) >= 0.5
    )
    cue_segments = len(cues)
    true_cue_segments = sum(
        1 for span in cues if covered_fraction(span, truth) >= 0.5
    )
    short_ms = short_utterance_ms / 1000
    short_truth = [span for span in truth if span[1] - span[0] <= short_ms]
    missed_short = sum(
        1 for span in short_truth if covered_fraction(span, cues) < 0.5
    )
    shouted = [
        (start, end)
        for start, end, text in (truth_entries or [])
        if is_shouted(text)
    ]
    missed_shouted = sum(
        1 for span in shouted if covered_fraction(span, cues) < 0.5
    )

    return {
        "truth_ms": round(truth_ms, 1),
        "cue_ms": round(cue_ms, 1),
        "covered_dialogue_ms": round(covered_ms, 1),
        "false_cue_ms": round(false_cue_ms, 1),
        "narratable_ms": round(narratable_ms, 1),
        "narration_window_ms": round(narration_ms, 1),
        "cue_vetoed_ms": round(cue_veto_ms, 1),
        "narratable_retained_ms": round(retained_ms, 1),
        "narration_on_dialogue_ms": round(leak_ms, 1),
        "scoring_mode": mode,
        "dialogue_time_recall": round(covered_ms / truth_ms, 4) if truth_ms else 1.0,
        "false_cue_coverage": round(false_cue_ms / cue_ms, 4) if cue_ms else 0.0,
        "dialogue_leak_ratio": round(leak_ms / leak_denom, 4) if leak_denom else 0.0,
        "narratable_retained_ratio": round(retained_ms / narratable_ms, 4)
        if narratable_ms
        else 0.0,
        "truth_segments": truth_segments,
        "covered_segments": covered_segments,
        "cue_segments": cue_segments,
        "true_cue_segments": true_cue_segments,
        "short_utterances": len(short_truth),
        "missed_short_utterances": missed_short,
        "shouted_utterances": len(shouted),
        "missed_shouted_utterances": missed_shouted,
        "dialogue_segment_recall": wilson(covered_segments, truth_segments),
        "cue_segment_precision": wilson(true_cue_segments, cue_segments),
    }


def cue_details(
    truth: list[tuple[float, float]],
    cues: list[tuple[float, float]],
    pad_s: float,
    source: str,
) -> list[dict]:
    truth = merge(truth)
    details = []
    for start, end in merge(pad_spans(cues, pad_s)):
        dialogue_ms = total(intersect([(start, end)], truth)) * 1000
        duration_ms = (end - start) * 1000
        details.append(
            {
                "start_s": round(start, 3),
                "end_s": round(end, 3),
                "source": source,
                "overlaps_dialogue": dialogue_ms > 0,
                "vetoed_narration": dialogue_ms < duration_ms,
                "false_cue_ms": round(duration_ms - dialogue_ms, 1),
            }
        )
    return details


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")


def evaluate_truth_files(
    truth_files: list[dict],
    cues: list[tuple[float, float]],
    total_s: float,
    pad_s: float,
    short_utterance_ms: int,
    detector_non_voice: list[tuple[float, float]] | None = None,
) -> dict:
    per_language = {}
    pooled_truth: list[tuple[float, float]] = []
    pooled_entries: list[tuple[float, float, str]] = []
    for item in truth_files:
        entries = parse_srt_entries(Path(item["path"]))
        spans = [(start, end) for start, end, _ in entries]
        pooled_truth.extend(spans)
        pooled_entries.extend(entries)
        per_language[item.get("language", "und")] = evaluate(
            spans, cues, total_s, pad_s, short_utterance_ms, entries, detector_non_voice
        )
    return {
        "per_language": per_language,
        "pooled": evaluate(
            pooled_truth,
            cues,
            total_s,
            pad_s,
            short_utterance_ms,
            pooled_entries,
            detector_non_voice,
        ),
    }


def check_bounds(metrics: dict, expect: dict) -> list[str]:
    failures = []
    for key, bounds in expect.items():
        if key not in metrics:
            failures.append(f"unknown metric '{key}'")
            continue
        value = metrics[key]
        if "min" in bounds and value < bounds["min"]:
            failures.append(f"{key}={value} < min {bounds['min']}")
        if "max" in bounds and value > bounds["max"]:
            failures.append(f"{key}={value} > max {bounds['max']}")
    return failures


def gate_failures(metrics: dict, gate: dict) -> list[str]:
    failures = []
    if "max_dialogue_leak_ratio" in gate:
        limit = gate["max_dialogue_leak_ratio"]
        if metrics["dialogue_leak_ratio"] > limit:
            failures.append(
                f"dialogue_leak_ratio={metrics['dialogue_leak_ratio']} > {limit}"
            )
    if "min_dialogue_time_recall" in gate:
        limit = gate["min_dialogue_time_recall"]
        if metrics["dialogue_time_recall"] < limit:
            failures.append(
                f"dialogue_time_recall={metrics['dialogue_time_recall']} < {limit}"
            )
    if "max_false_cue_coverage" in gate:
        limit = gate["max_false_cue_coverage"]
        if metrics["false_cue_coverage"] > limit:
            failures.append(
                f"false_cue_coverage={metrics['false_cue_coverage']} > {limit}"
            )
    return failures


def run_fixtures(manifest_path: Path, output: Path) -> int:
    manifest = load_json(manifest_path)
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        print("fixture manifest must contain a non-empty 'cases' list", file=sys.stderr)
        return 1

    results = []
    all_passed = True
    for case in cases:
        case_id = case["id"]
        truth_files = case["truth"]
        total_s = case.get("total_ms", 0) / 1000
        if total_s <= 0:
            ends = [end for item in truth_files for _, end in parse_srt(Path(item["path"]))]
            for source in case["cues"]:
                ends.extend(end for _, end in parse_srt(Path(source["path"])))
            total_s = max(ends, default=0.0) + 1.0
        pad_s = case.get("pad_s", 0.0)
        short_ms = case.get("short_utterance_ms", 1000)

        source_results = []
        for source in case["cues"]:
            cues = parse_srt(Path(source["path"]))
            scored = evaluate_truth_files(truth_files, cues, total_s, pad_s, short_ms)
            scored["source"] = source["source"]
            pooled_truth = [
                span
                for item in truth_files
                for span in parse_srt(Path(item["path"]))
            ]
            scored["cue_details"] = cue_details(
                pooled_truth, cues, pad_s, source["source"]
            )
            source_results.append(scored)

        primary = source_results[0]["pooled"]
        failures = check_bounds(primary, case.get("expect", {}))
        gate = case.get("gate")
        gate_passed = True
        if gate:
            gate_failures_list = gate_failures(primary, gate)
            gate_passed = not gate_failures_list
            expected = case.get("expect_gate", "pass")
            if expected == "pass" and not gate_passed:
                failures.extend(gate_failures_list)
            elif expected == "fail" and gate_passed:
                failures.append("gate passed but the fixture expects a regression")
        if failures:
            all_passed = False
        results.append(
            {
                "id": case_id,
                "passed": not failures,
                "failures": failures,
                "gate_passed": gate_passed,
                "expect_gate": case.get("expect_gate", "pass"),
                "sources": source_results,
            }
        )

    payload = {
        "manifest": str(manifest_path),
        "case_count": len(results),
        "passed": all_passed,
        "cases": results,
    }
    write_json(output, payload)
    for result in results:
        status = "PASS" if result["passed"] else "FAIL"
        print(f"{status} {result['id']}")
        for failure in result["failures"]:
            print(f"  - {failure}", file=sys.stderr)
    print(f"cue-path fixtures: {len(results)} cases, passed={all_passed}")
    return 0 if all_passed else 1


def run_film(args: argparse.Namespace) -> int:
    truth_files = [{"path": path, "language": args.language} for path in args.truth]
    cues = parse_srt(Path(args.cues))
    detector = (
        load_detector_non_voice(Path(args.detector_timeline))
        if args.detector_timeline
        else None
    )
    total_s = args.total_ms / 1000
    if total_s <= 0:
        ends = [end for item in truth_files for _, end in parse_srt(Path(item["path"]))]
        ends.extend(end for _, end in cues)
        total_s = max(ends, default=0.0) + 1.0
    scored = evaluate_truth_files(
        truth_files, cues, total_s, args.pad, args.short_utterance_ms, detector
    )
    pooled_truth = [
        span for item in truth_files for span in parse_srt(Path(item["path"]))
    ]
    payload = {
        "cue_source": args.cue_source,
        "pad_s": args.pad,
        "total_ms": round(total_s * 1000),
        "detector_timeline": args.detector_timeline,
        "cue_details": cue_details(pooled_truth, cues, args.pad, args.cue_source),
        **scored,
    }
    write_json(Path(args.output), payload)
    print(json.dumps(payload["pooled"], indent=2, sort_keys=True))
    return 0


def derive_cues(
    media: Path,
    out: Path,
    model: str,
    revision: str,
    threshold: float,
) -> bool:
    cmd = [
        sys.executable,
        "scripts/transcribe_cues.py",
        str(media),
        str(out),
        "--model",
        model,
        "--model-revision",
        revision,
        "--min-vad-evidence",
        str(threshold),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(
            f"cue derivation failed for {media}: {result.stderr.strip()}",
            file=sys.stderr,
        )
        return False
    return True


def select_config(dev_results: list[dict], max_leak: float) -> dict:
    eligible = [
        row for row in dev_results if row["metrics"]["dialogue_leak_ratio"] <= max_leak
    ]
    pool = eligible or dev_results
    return max(
        pool,
        key=lambda row: (
            row["metrics"]["narratable_retained_ms"],
            -row["metrics"]["dialogue_leak_ratio"],
        ),
    )


def select_per_source(dev_rows: list[dict], max_leak: float) -> dict:
    """Pick one config per cue source: sources are not alternatives to rank."""
    by_source: dict[str, list[dict]] = {}
    for row in dev_rows:
        by_source.setdefault(row["source"], []).append(row)
    return {source: select_config(rows, max_leak) for source, rows in by_source.items()}


def run_sweep(args: argparse.Namespace) -> int:
    manifest = load_json(Path(args.manifest))
    entries = manifest.get("entries")
    if not isinstance(entries, list) or not entries:
        print("sweep manifest must contain a non-empty 'entries' list", file=sys.stderr)
        return 1
    pads = manifest.get("pads_s", [0.0])
    thresholds = manifest.get("min_vad_evidence", [0.0])
    short_ms = manifest.get("short_utterance_ms", 1000)
    max_leak = manifest.get("max_dialogue_leak_ratio", 0.05)

    rows = []
    pending = []
    ready: list[tuple[dict, list | None, float, Path]] = []
    cache_dir = Path(args.cue_cache_dir) if args.cue_cache_dir else None
    if cache_dir is not None:
        cache_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="cue-eval-") as tmp:
        for entry in entries:
            if entry.get("status", "ready") != "ready":
                pending.append(
                    {
                        "id": entry["id"],
                        "status": entry.get("status"),
                        "split": entry.get("split", "dev"),
                    }
                )
                continue
            media = Path(entry["media"])
            if not media.exists():
                pending.append(
                    {
                        "id": entry["id"],
                        "status": "missing_media",
                        "split": entry.get("split", "dev"),
                    }
                )
                continue
            missing_truth = [
                item["path"] for item in entry["truth"] if not Path(item["path"]).exists()
            ]
            if missing_truth:
                pending.append(
                    {
                        "id": entry["id"],
                        "status": "missing_truth",
                        "split": entry.get("split", "dev"),
                        "paths": missing_truth,
                    }
                )
                continue
            detector = None
            timeline = entry.get("detector_timeline")
            if timeline:
                if not Path(timeline).exists():
                    pending.append(
                        {
                            "id": entry["id"],
                            "status": "missing_detector_timeline",
                            "split": entry.get("split", "dev"),
                            "paths": [timeline],
                        }
                    )
                    continue
                detector = load_detector_non_voice(Path(timeline))
            total_s = entry["total_ms"] / 1000
            ready.append((entry, detector, total_s, media))
            for source in entry["cue_sources"]:
                if source.get("derive"):
                    if not args.derive_cues:
                        continue
                    for threshold in thresholds:
                        name = f"{entry['id']}-{threshold}.srt"
                        out = (cache_dir or Path(tmp)) / name
                        # Reuse a cached derivation so an interrupted sweep resumes.
                        if out.exists() and out.stat().st_size > 0:
                            print(f"reusing cached cues {out}", file=sys.stderr)
                        elif not derive_cues(
                            media, out, args.model, args.model_revision, threshold
                        ):
                            continue
                        cues = parse_srt(out)
                        for pad in pads:
                            scored = evaluate_truth_files(
                                entry["truth"], cues, total_s, pad, short_ms, detector
                            )
                            rows.append(
                                {
                                    "id": entry["id"],
                                    "split": entry.get("split", "dev"),
                                    "source": source["source"],
                                    "pad_s": pad,
                                    "min_vad_evidence": threshold,
                                    "metrics": scored["pooled"],
                                    "per_language": scored["per_language"],
                                    "cue_details": cue_details(
                                        [
                                            span
                                            for item in entry["truth"]
                                            for span in parse_srt(Path(item["path"]))
                                        ],
                                        cues,
                                        pad,
                                        source["source"],
                                    )
                                    if args.cue_details
                                    else None,
                                }
                            )
                else:
                    cues = parse_srt(Path(source["path"]))
                    for pad in pads:
                        scored = evaluate_truth_files(
                            entry["truth"], cues, total_s, pad, short_ms, detector
                        )
                        rows.append(
                            {
                                "id": entry["id"],
                                "split": entry.get("split", "dev"),
                                "source": source["source"],
                                "pad_s": pad,
                                "min_vad_evidence": None,
                                "metrics": scored["pooled"],
                                "per_language": scored["per_language"],
                                "cue_details": cue_details(
                                    [
                                        span
                                        for item in entry["truth"]
                                        for span in parse_srt(Path(item["path"]))
                                    ],
                                    cues,
                                    pad,
                                    source["source"],
                                )
                                if args.cue_details
                                else None,
                            }
                        )

        # The product hardcodes its pad (DERIVED_CUE_PAD_MS) and never passes an
        # evidence floor, so the dev-selected config is *not* what ships. Report
        # the shipped configuration explicitly instead of implying tuning lands.
        shipped_rows = []
        shipped_cfg = manifest.get("shipped_config")
        if shipped_cfg and args.derive_cues:
            shipped_floor = shipped_cfg.get("min_vad_evidence")
            shipped_pad = shipped_cfg["pad_s"]
            for entry, detector, total_s, media in ready:
                out = (cache_dir or Path(tmp)) / f"{entry['id']}-{shipped_floor}.srt"
                if not out.exists() or out.stat().st_size == 0:
                    if not derive_cues(
                        media, out, args.model, args.model_revision, shipped_floor
                    ):
                        continue
                cues = parse_srt(out)
                scored = evaluate_truth_files(
                    entry["truth"], cues, total_s, shipped_pad, short_ms, detector
                )
                shipped_rows.append(
                    {
                        "id": entry["id"],
                        "split": entry.get("split", "dev"),
                        "pad_s": shipped_pad,
                        "min_vad_evidence": shipped_floor,
                        "metrics": scored["pooled"],
                        "per_language": scored["per_language"],
                    }
                )

    dev_rows = [row for row in rows if row["split"] == "dev"]
    if not dev_rows:
        print("sweep produced no dev rows; cannot select a config", file=sys.stderr)
        return 1
    chosen = select_per_source(dev_rows, max_leak)
    holdout = [
        row
        for row in rows
        if row["split"] == "holdout"
        and row["source"] in chosen
        and row["pad_s"] == chosen[row["source"]]["pad_s"]
        and row["min_vad_evidence"] == chosen[row["source"]]["min_vad_evidence"]
    ]
    payload = {
        "manifest": str(args.manifest),
        "max_dialogue_leak_ratio": max_leak,
        "shipped_config": shipped_cfg,
        "selected": {
            source: {
                "pad_s": row["pad_s"],
                "min_vad_evidence": row["min_vad_evidence"],
                "dev_film": row["id"],
                "dev_metrics": row["metrics"],
            }
            for source, row in chosen.items()
        },
        "pending": pending,
        "dev": dev_rows,
        "holdout": holdout,
        "shipped": shipped_rows,
    }
    write_json(Path(args.output), payload)
    for source, row in sorted(chosen.items()):
        print(
            f"selected {source} pad={row['pad_s']} "
            f"threshold={row['min_vad_evidence']} on dev"
        )
    for row in holdout:
        metrics = row["metrics"]
        print(
            f"holdout {row['id']}: leak={metrics['dialogue_leak_ratio']} "
            f"recall={metrics['dialogue_time_recall']} "
            f"retained={metrics['narratable_retained_ms']}ms"
        )
    for row in shipped_rows:
        metrics = row["metrics"]
        print(
            f"shipped {row['id']}: leak={metrics['dialogue_leak_ratio']} "
            f"recall={metrics['dialogue_time_recall']} "
            f"retained={metrics['narratable_retained_ms']}ms"
        )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    film = sub.add_parser("film", help="score one film")
    film.add_argument("--truth", action="append", required=True)
    film.add_argument("--cues", required=True)
    film.add_argument("--cue-source", default="unknown")
    film.add_argument(
        "--detector-timeline",
        default=None,
        help="timeline JSON; narration windows become its non-voice segments minus cues",
    )
    film.add_argument("--language", default="und")
    film.add_argument("--total-ms", type=int, default=0)
    film.add_argument("--pad", type=float, default=0.0)
    film.add_argument("--short-utterance-ms", type=int, default=1000)
    film.add_argument("--output", required=True)
    film.set_defaults(func=run_film)

    fixtures = sub.add_parser("fixtures", help="run deterministic regression fixtures")
    fixtures.add_argument("--manifest", default="testdata/cues/fixtures.json")
    fixtures.add_argument(
        "--output", default="analysis/validation/cue-path-fixtures.json"
    )

    fixtures.set_defaults(func=lambda a: run_fixtures(Path(a.manifest), Path(a.output)))

    sweep = sub.add_parser("sweep", help="evaluate pad/threshold configs")
    sweep.add_argument("--manifest", required=True)
    sweep.add_argument("--output", required=True)
    sweep.add_argument("--derive-cues", action="store_true")
    sweep.add_argument("--model", default="base")
    sweep.add_argument("--model-revision", default=DEFAULT_MODEL_REVISION)
    sweep.add_argument(
        "--cue-details",
        action="store_true",
        help="include per-cue source and veto status in the sweep report",
    )
    sweep.add_argument(
        "--cue-cache-dir",
        default=None,
        help="keep derived cues here so an interrupted sweep resumes instead of re-transcribing",
    )
    sweep.set_defaults(func=run_sweep)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
