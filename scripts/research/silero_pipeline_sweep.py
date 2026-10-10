#!/usr/bin/env python3
"""Sweep the pipeline settings that sit between the Silero probabilities and the gaps.

The shipped detector is the default AnalysisConfig + Silero. Silero gives some
dialogue (shouts over action) probabilities of 0.4-0.9, but the smoothing and
merging stages decide whether those frames survive. This sweeps SILERO_VAD_THRESHOLD,
speech hangover, merge gap and minimum speech length on all three films and reports
the dev winner against the holdouts, so a setting is not picked on one film.

Score per film = harmonic mean of speech-time recall, non-voice time precision and
non-voice time recall (see profile_sweep.py). Usage: silero_pipeline_sweep.py [--out F]
"""
import argparse
import itertools
import json
import os
import subprocess
import tempfile
from pathlib import Path

FILMS = {
    "ED": ("testdata/raw/elephants_dream_2006.mp4", "testdata/raw/elephants_dream_2006.de.srt", 653791),
    "ToS": ("testdata/raw/tears_of_steel_2012.mov", "testdata/raw/tears_of_steel_2012.en.srt", 734167),
    "Sintel": ("testdata/raw/Sintel.2010.720p.mkv", "testdata/raw/sintel_2010.en.srt", 888032),
}
BASE = {
    "sample_rate_hz": 16000, "frame_ms": 20, "speech_hangover_ms": 300, "merge_gap_ms": 250,
    "min_speech_ms": 120, "min_non_voice_ms": 10000, "max_non_voice_ms": None, "energy_threshold": 0.015,
    "vad_threshold_delta": 0.0, "prompt_min_duration_ms": 2500, "prompt_min_confidence": 0.65,
    "vad_engine": "silero",
}
BASELINE = (0.3, 300, 250, 120)  # shipped: SILERO_VAD_THRESHOLD default, default AnalysisConfig
GRID = {
    "thr": [0.15, 0.3, 0.5],
    "hang": [150, 300, 600],
    "gap": [250, 800],
    "minsp": [60, 120],
}


def hmean(v):
    return 0.0 if any(x <= 0 for x in v) else len(v) / sum(1 / x for x in v)


def score(binary, film, overrides, thr):
    media, srt, total = FILMS[film]
    cfg = {**BASE, **overrides}
    with tempfile.TemporaryDirectory() as d:
        cfg_path, out = Path(d, "c.json"), Path(d, "o.json")
        cfg_path.write_text(json.dumps(cfg))
        env = {**os.environ, "SILERO_VAD_THRESHOLD": str(thr)}
        subprocess.run([binary, "validate", media, "--subtitles", srt, "--total-ms", str(total),
                        "--vad-engine", "silero", "--config", str(cfg_path), "--output", str(out)], check=True, capture_output=True, env=env)
        m = json.loads(out.read_text())
    nv_p, nv_r, sp_r = m["non_voice_time_precision"], m["non_voice_time_recall"], m["speech_time_recall"]
    return {"sp_r": sp_r, "nv_p": nv_p, "nv_r": nv_r, "score": hmean([sp_r, nv_p, nv_r])}


def summarize(rows):
    """Pick the candidate with the best dev score and report where it lands on the holdouts.

    Ties on dev break toward the baseline-like candidate (first in grid order), so a flat
    grid reports the earliest setting rather than an arbitrary one.
    """
    dev_winner = max(rows, key=lambda r: r["dev"])
    by_holdout = sorted(rows, key=lambda r: -r["holdout"])
    baseline = next((r for r in rows if (r["thr"], r["hang"], r["gap"], r["minsp"]) == BASELINE), None)
    key = lambda r: {k: r[k] for k in ("thr", "hang", "gap", "minsp")}
    return {
        "dev_winner": {**key(dev_winner), "dev": dev_winner["dev"], "holdout": dev_winner["holdout"]},
        "dev_winner_holdout_rank": f"{by_holdout.index(dev_winner) + 1}/{len(rows)}",
        "holdout_winner": {**key(by_holdout[0]), "dev": by_holdout[0]["dev"], "holdout": by_holdout[0]["holdout"]},
        "shipped_baseline": baseline and {"dev": baseline["dev"], "holdout": baseline["holdout"]},
        "holdout_range": [min(r["holdout"] for r in rows), max(r["holdout"] for r in rows)],
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="target/debug/timeline")
    ap.add_argument("--out", default="silero_sweep.json")
    args = ap.parse_args()
    rows = []
    for thr, hang, gap, minsp in itertools.product(*GRID.values()):
        ov = {"speech_hangover_ms": hang, "merge_gap_ms": gap, "min_speech_ms": minsp}
        per = {f: score(args.binary, f, ov, thr) for f in FILMS}
        rows.append({"thr": thr, "hang": hang, "gap": gap, "minsp": minsp, "films": per,
                     "dev": per["ED"]["score"], "holdout": (per["ToS"]["score"] + per["Sintel"]["score"]) / 2})
        print(rows[-1]["thr"], hang, gap, minsp, "dev %.3f holdout %.3f" % (rows[-1]["dev"], rows[-1]["holdout"]), flush=True)
    summary = summarize(rows)
    Path(args.out).write_text(json.dumps({"summary": summary, "rows": rows}, indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
