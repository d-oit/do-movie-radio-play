#!/usr/bin/env python3
"""Does a second, more sensitive Silero pass close the narration-over-dialogue gap?

Short answer: no. Recorded here so the idea is not re-proposed from scratch.

`radio-play` already narrates only where the detector and the cues agree on
non-speech (`subtract_cues(gaps = detector non-voice, cues)` in
`crates/movie-radio-goap/src/gaps/split.rs`), so "agreement" is the shipped
rule. The remaining leakage is dialogue that *both* signals missed. This
experiment tests whether re-running the same model at a lower threshold —
giving a more sensitive speech set to veto with — recovers any of it.

Inputs: the shipped-threshold timeline, a lower-threshold timeline
(`SILERO_VAD_THRESHOLD=0.1`, note the engine ignores `--threshold` because that
knob is energy RMS), the ungated cue file, and the truth subtitles.

Usage:
  double_veto.py --truth T.srt --cues C.srt --total-ms N \
                 --timeline shipped.json --timeline-low sensitive.json
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from evaluate_cue_path import (  # noqa: E402
    evaluate_truth_files,
    load_detector_non_voice,
    parse_srt,
    subtract,
)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--truth", required=True)
    ap.add_argument("--cues", required=True)
    ap.add_argument("--total-ms", type=int, required=True)
    ap.add_argument("--timeline", required=True, help="shipped threshold timeline")
    ap.add_argument("--timeline-low", required=True, help="lower threshold timeline")
    ap.add_argument("--pad", type=float, default=0.8)
    args = ap.parse_args()

    total_s = args.total_ms / 1000
    truth = [{"path": args.truth, "language": "und"}]
    cues = parse_srt(Path(args.cues))
    shipped = load_detector_non_voice(Path(args.timeline))

    base = evaluate_truth_files(truth, cues, total_s, args.pad, 1000, shipped)["pooled"]
    # The extract output only carries non-voice segments, so the sensitive pass's
    # speech set is the complement of its non-voice set within the film.
    low_nv = load_detector_non_voice(Path(args.timeline_low))
    low_speech = subtract([(0.0, total_s)], low_nv)
    vetoed = sorted(set(cues) | set(low_speech))
    with_veto = evaluate_truth_files(truth, vetoed, total_s, args.pad, 1000, shipped)["pooled"]

    gained_s = (base["narratable_retained_ms"] - with_veto["narratable_retained_ms"]) / 1000
    print(f"shipped           leak={base['dialogue_leak_ratio']*100:5.1f}%  "
          f"retained={base['narratable_retained_ms']/1000:7.1f}s")
    print(f"+silero@low       leak={with_veto['dialogue_leak_ratio']*100:5.1f}%  "
          f"retained={with_veto['narratable_retained_ms']/1000:7.1f}s  "
          f"(second pass vetoed {gained_s:.1f}s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
