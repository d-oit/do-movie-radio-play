#!/usr/bin/env python3
"""Decide which speech-to-text cues survive the Silero evidence gate.

Kept separate from `transcribe_cues.py` so the policy is testable without the
model (`scripts/test_cue_gate.py`, PR CI).

Why duration matters: Whisper hallucinates speech over score in long, sustained
passages, which is what this gate was built to remove. A short cue is a
different risk — if it is wrong it costs at most its own duration of narration,
but if it is dropped it can hide a shouted line entirely. Measured on four films
(pad 0.8 s), gating every cue regardless of length dropped 13 of 34 shouts on
Sprite Fright and leaked 13.9% of retained narratable time; gating only cues
longer than 10 s cut that to 4 shouts and 11.3%.
"""

DEFAULT_GATE_MAX_CUE_SECONDS = 10.0


def keep_cue(
    start: float, end: float, peak_evidence: float, floor: float, max_gated_seconds: float
) -> bool:
    """True when the cue survives the gate."""
    if floor <= 0:
        return True
    if (end - start) <= max_gated_seconds:
        return True
    return peak_evidence >= floor


def peak_evidences(probs, step: float, cues) -> list[float]:
    """Peak speech probability inside each cue, one value per cue."""
    peaks = []
    for start, end in cues:
        lo = int(start / step)
        hi = max(int(end / step), lo + 1)
        window = probs[lo:hi]
        peaks.append(float(window.max()) if len(window) else 0.0)
    return peaks
