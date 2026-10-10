#!/usr/bin/env python3
"""Dump raw faster-whisper segments (timings + confidences) as JSON so cue filters can be tuned offline.
"""
import argparse
import json
import sys

from faster_whisper import WhisperModel

ap = argparse.ArgumentParser(description="Dump raw faster-whisper segments as JSON")
ap.add_argument("media")
ap.add_argument("out")
args = ap.parse_args()
model = WhisperModel("base", device="cpu", compute_type="int8")
segs, _ = model.transcribe(args.media, vad_filter=False, word_timestamps=True, condition_on_previous_text=False, temperature=0.0)
out = []
for s in segs:
    out.append({"start": s.start, "end": s.end, "no_speech": s.no_speech_prob, "logprob": s.avg_logprob,
                "comp": s.compression_ratio,
                "words": [{"s": w.start, "e": w.end, "p": w.probability} for w in (s.words or [])]})
json.dump(out, open(args.out, "w"))
print(len(out), "segments", file=sys.stderr)
