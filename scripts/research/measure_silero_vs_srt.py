#!/usr/bin/env python3
"""#362: time-domain speech/non-voice P/R of Silero VAD vs a timed SRT (needs silero-vad, numpy)."""
import re, subprocess, sys
import numpy as np
from silero_vad import get_speech_timestamps, load_silero_vad

media, srt = sys.argv[1:3]
thr = float(sys.argv[3]) if len(sys.argv) > 3 else 0.5
raw = subprocess.run(["ffmpeg", "-v", "quiet", "-i", media, "-ac", "1", "-ar", "16000", "-f", "f32le", "-"], capture_output=True).stdout
import torch
x = torch.from_numpy(np.frombuffer(raw, dtype=np.float32).copy()); sr = 16000
n = len(x) // 160
def t(s):
    h, m, r = s.replace(",", ".").split(":"); return int(h) * 3600 + int(m) * 60 + float(r)
truth = np.zeros(n, bool)
for a, b in re.findall(r"(\d\d:\d\d:\d\d,\d+) --> (\d\d:\d\d:\d\d,\d+)", open(srt).read()):
    truth[int(t(a) * 100):int(t(b) * 100)] = True
pred = np.zeros(n, bool)
for ts in get_speech_timestamps(x, load_silero_vad(), threshold=thr, sampling_rate=sr, min_silence_duration_ms=300):
    pred[ts["start"] // 160:ts["end"] // 160] = True
def pr(p, tr): 
    tp = (p & tr).sum(); return tp / max(p.sum(), 1), tp / max(tr.sum(), 1)
sp, sr_ = pr(pred, truth); np_, nr = pr(~pred, ~truth)
print(f"thr={thr} speech P/R {sp:.3f}/{sr_:.3f}  non-voice P/R {np_:.3f}/{nr:.3f}")
