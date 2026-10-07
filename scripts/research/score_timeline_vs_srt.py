#!/usr/bin/env python3
"""Time-domain speech / non-voice P/R of a timeline JSON against a timed SRT (stdlib only)."""
import json, re, sys

def t(s):
    h, m, r = s.replace(",", ".").split(":"); return int(h) * 3600 + int(m) * 60 + float(r)

tl = json.load(open(sys.argv[1]))
n = int(max(s["end_ms"] for s in tl["segments"]) / 10) + 6000
truth = [False] * n
for a, b in re.findall(r"(\d\d:\d\d:\d\d,\d+) --> (\d\d:\d\d:\d\d,\d+)", open(sys.argv[2]).read()):
    for i in range(int(t(a) * 100), min(int(t(b) * 100), n)): truth[i] = True
pred = [True] * n
for s in tl["segments"]:
    if s["kind"] == "non_voice":
        for i in range(s["start_ms"] // 10, min(s["end_ms"] // 10, n)): pred[i] = False
def pr(p, q):
    tp = sum(1 for a, b in zip(p, q) if a and b); return tp / max(sum(p), 1), tp / max(sum(q), 1)
neg = lambda v: [not x for x in v]
print("speech P/R %.3f/%.3f  non-voice P/R %.3f/%.3f  non-voice segments %d" % (*pr(pred, truth), *pr(neg(pred), neg(truth)), sum(s["kind"] == "non_voice" for s in tl["segments"])))
