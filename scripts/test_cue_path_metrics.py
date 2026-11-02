#!/usr/bin/env python3
"""Tests for the cue-path scoring semantics (no media, no model).

The important invariant: narration windows are the detector's non-voice time
minus cues, so scored leak can never include dialogue the detector already
refused to narrate.
"""

import sys

from evaluate_cue_path import evaluate, subtract

FAILURES: list[str] = []


def check(label: str, got, want) -> None:
    if got != want:
        FAILURES.append(f"{label}: got {got!r}, want {want!r}")


def main() -> int:
    check("subtract middle", subtract([(0, 10)], [(3, 5)]), [(0, 3), (5, 10)])
    check("subtract edge", subtract([(0, 10)], [(0, 2)]), [(2, 10)])
    check("subtract none", subtract([(0, 10)], [(20, 30)]), [(0, 10)])
    check("subtract covering", subtract([(0, 10)], [(0, 30)]), [])
    check("subtract splits twice", subtract([(0, 10)], [(2, 4), (6, 8)]),
          [(0, 2), (4, 6), (8, 10)])

    # Dialogue 0-2 s, cue 3-5 s, detector correctly calling 2-4 s speech.
    truth = [(0.0, 2.0)]
    cues = [(3.0, 5.0)]
    detector = [(2.0, 4.0), (6.0, 10.0)]
    scored = evaluate(truth, cues, 10.0, 0.0, 1000, None, detector)
    check("mode", scored["scoring_mode"], "detector_plus_cues")
    # Detector non-voice is 2 s + 4 s.
    check("narratable is detector non-voice", scored["narratable_ms"], 6000.0)
    # The cue removes 3-4 s of that, leaving 2-3 s and 6-10 s.
    check("narration windows exclude cues", scored["narration_window_ms"], 5000.0)
    check("no leak when detector excludes dialogue", scored["dialogue_leak_ratio"], 0.0)
    check("cue veto counted", scored["cue_vetoed_ms"], 1000.0)

    # Same cue, but the detector wrongly narrates the dialogue range 0-2 s.
    wrong = [(0.0, 4.0), (6.0, 10.0)]
    scored = evaluate(truth, cues, 10.0, 0.0, 1000, None, wrong)
    check("detector leak surfaces", scored["narration_on_dialogue_ms"], 2000.0)
    # Narration is 0-3 s plus 6-10 s = 7 s, of which 2 s is dialogue.
    check("leak ratio", scored["dialogue_leak_ratio"], 0.2857)

    # Without a timeline the mode is labelled as an upper bound, not the shipped path.
    legacy = evaluate(truth, cues, 10.0, 0.0, 1000)
    check("legacy mode label", legacy["scoring_mode"], "cue_only_upper_bound")

    if FAILURES:
        print("cue-path metric tests failed:", file=sys.stderr)
        for failure in FAILURES:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print("cue-path metric tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
