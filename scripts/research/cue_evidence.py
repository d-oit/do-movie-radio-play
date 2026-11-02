#!/usr/bin/env python3
"""Dump per-cue peak Silero speech probability so gate policies can be scored offline.

`transcribe_cues.py` drops a whole cue when Silero never reaches the evidence
floor inside it. That is duration-blind: a 1 s shout over an explosion and a
20 s hallucinated musical passage are treated the same way. This tool records
the per-cue evidence once, so candidate policies (duration-aware gates) can be
compared without re-transcribing the films.

Input is the *ungated* cue SRT (`--min-vad-evidence 0`), which is the exact cue
list the gate sees. Output is JSON: cue spans with their peak probability.

Usage: cue_evidence.py MEDIA CUES.srt OUT.json
"""

import argparse
import json
import re
import sys

import numpy as np
from faster_whisper.audio import decode_audio
from faster_whisper.vad import get_vad_model

TIMESTAMP = re.compile(
    r"(\d\d):(\d\d):(\d\d)[,.](\d+)\s*-->\s*(\d\d):(\d\d):(\d\d)[,.](\d+)"
)
WINDOW = 512


def parse_srt(path: str) -> list[tuple[float, float]]:
    text = open(path, encoding="utf-8-sig", errors="ignore").read().replace("\r\n", "\n")
    spans = []
    for match in TIMESTAMP.finditer(text):
        h1, m1, s1, f1, h2, m2, s2, f2 = match.groups()
        start = int(h1) * 3600 + int(m1) * 60 + int(s1) + int(f1) / 1000
        end = int(h2) * 3600 + int(m2) * 60 + int(s2) + int(f2) / 1000
        if end > start:
            spans.append((start, end))
    return spans


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("media")
    ap.add_argument("cues")
    ap.add_argument("out")
    args = ap.parse_args()

    audio = decode_audio(args.media, sampling_rate=16000)
    usable = len(audio) // WINDOW * WINDOW
    probs = np.asarray(
        get_vad_model()(audio[:usable], num_samples=WINDOW)
    ).reshape(-1)
    step = WINDOW / 16000

    rows = []
    for start, end in parse_srt(args.cues):
        lo = int(start / step)
        hi = max(int(end / step), lo + 1)
        peak = float(probs[lo:hi].max(initial=0.0))
        rows.append(
            {"start": round(start, 3), "end": round(end, 3), "peak_evidence": round(peak, 4)}
        )
    json.dump({"media": args.media, "cue_count": len(rows), "cues": rows},
              open(args.out, "w"), indent=2, sort_keys=True)
    print(f"{len(rows)} cues -> {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
