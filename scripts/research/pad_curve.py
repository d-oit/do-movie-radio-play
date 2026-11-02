#!/usr/bin/env python3
"""Leak vs coverage as the cue pad grows, on the shipped path.

Answers one question: can a larger pad alone get an unseen film under the 5%
floor without throwing away the narration? Measured 2026-11-02: it can, but
only by narrating roughly a third less, and choosing that pad from the unseen
film's own result would be holdout fitting.

Also records a refuted alternative: merging cues separated by small gaps (to
cover a missed line sitting between two transcribed ones) leaks *more* than the
uniform pad at the same coverage, so it is not a cheaper route to the floor.

Usage:
  pad_curve.py --manifest testdata/validation/cue-eval-manifest.json \
               --cue-cache-dir /tmp/cuecache --floor 0.1
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from evaluate_cue_path import (  # noqa: E402
    evaluate_truth_files,
    load_detector_non_voice,
    parse_srt,
)

DEFAULT_PADS = [0.8, 1.2, 1.6, 2.0, 2.5, 3.0, 4.0]


def fill_gaps(spans, gap: float):
    """Merge cues separated by at most `gap` seconds."""
    out = []
    for start, end in sorted(spans):
        if out and start - out[-1][1] <= gap:
            out[-1] = (out[-1][0], max(out[-1][1], end))
        else:
            out.append((start, end))
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--manifest", default="testdata/validation/cue-eval-manifest.json")
    ap.add_argument("--cue-cache-dir", required=True)
    ap.add_argument("--floor", default="0.1")
    ap.add_argument("--fill-gap", type=float, default=None,
                    help="also test merging cues closer than this before padding")
    args = ap.parse_args()

    import json

    manifest = json.loads(Path(args.manifest).read_text(encoding="utf-8"))
    entries = []
    for entry in manifest.get("entries", []):
        timeline = entry.get("detector_timeline")
        cues = Path(args.cue_cache_dir) / f"{entry['id']}-{args.floor}.srt"
        if not timeline or not Path(timeline).exists() or not cues.exists():
            continue
        entries.append((entry["id"], entry, cues))
    if not entries:
        print("no entries with both a detector timeline and cached cues", file=sys.stderr)
        return 1

    header = " | ".join(f"{name[:13]:>13s}" for name, _, _ in entries)
    print("leak% / retained% per film")
    print(f"{'pad':>5s} | {header}")
    for pad in DEFAULT_PADS:
        cells = []
        for _, entry, cues_path in entries:
            detector = load_detector_non_voice(Path(entry["detector_timeline"]))
            cues = parse_srt(cues_path)
            scored = evaluate_truth_files(
                entry["truth"], cues, entry["total_ms"] / 1000, pad, 1000, detector
            )["pooled"]
            cells.append(
                f"{scored['dialogue_leak_ratio']*100:5.1f}/{scored['narratable_retained_ratio']*100:5.1f}"
            )
        print(f"{pad:5.1f} | " + " | ".join(f"{c:>13s}" for c in cells))

    if args.fill_gap:
        print()
        print(f"fill cues closer than {args.fill_gap}s, then pad 0.3s")
        cells = []
        for _, entry, cues_path in entries:
            detector = load_detector_non_voice(Path(entry["detector_timeline"]))
            filled = fill_gaps(parse_srt(cues_path), args.fill_gap)
            scored = evaluate_truth_files(
                entry["truth"], filled, entry["total_ms"] / 1000, 0.3, 1000, detector
            )["pooled"]
            cells.append(
                f"{scored['dialogue_leak_ratio']*100:5.1f}/{scored['narratable_retained_ratio']*100:5.1f}"
            )
        print(f"{'fill':>5s} | " + " | ".join(f"{c:>13s}" for c in cells))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
