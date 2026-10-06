#!/usr/bin/env python3
"""Dialogue-time precision/recall of auto cues vs truth SRT (stdlib). Recall is what matters:
 a missed dialogue second is a second narration could land on."""
import re, sys
def spans(p):
    def t(s):
        h, m, r = s.replace(",", ".").split(":"); return int(h) * 3600 + int(m) * 60 + float(r)
    txt = open(p, encoding="utf-8", errors="ignore").read()
    return [(t(a), t(b)) for a, b in re.findall(r"(\d\d:\d\d:\d\d[,.]\d+) --> (\d\d:\d\d:\d\d[,.]\d+)", txt)]
def grid(sp, n, pad=0.0):
    g = [False] * n
    for a, b in sp:
        for i in range(max(int((a - pad) * 100), 0), min(int((b + pad) * 100) + 1, n)): g[i] = True
    return g
truth, auto = spans(sys.argv[1]), spans(sys.argv[2])
pad = float(sys.argv[3]) if len(sys.argv) > 3 else 0.0
n = int(max(b for _, b in truth + auto) * 100) + 100
T, A = grid(truth, n), grid(auto, n, pad)
tp = sum(1 for x, y in zip(T, A) if x and y)
print(f"pad={pad}s  dialogue recall {tp/max(sum(T),1):.3f}  precision {tp/max(sum(A),1):.3f}  (truth {sum(T)/100:.0f}s, auto {sum(A)/100:.0f}s)")
