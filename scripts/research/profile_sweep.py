#!/usr/bin/env python3
"""#364 step 3: does the dev winner of a 16-candidate sweep survive the holdouts?

Runs `timeline validate` for every (energy threshold, min silence) candidate on the
dev film and the holdout films, scores each film with the harmonic mean of
speech-time recall and non-voice time precision/recall (the three numbers that decide
whether narration lands in real gaps), then compares the dev winner with the best
holdout candidate. Also scores the shipped detector (default config + Silero).

Needs the fetched films (scripts/fetch_test_assets.sh with FETCH_SECOND_FILM=1).
Usage: profile_sweep.py [--binary target/debug/timeline] [--out sweep.json]
"""
import argparse
import itertools
import json
import subprocess
import tempfile
from pathlib import Path

FILMS = {
    "dev": [("elephants_dream", "testdata/raw/elephants_dream_2006.mp4", "testdata/raw/elephants_dream_2006.de.srt", 653791)],
    "holdout": [
        ("tears_of_steel", "testdata/raw/tears_of_steel_2012.mov", "testdata/raw/tears_of_steel_2012.en.srt", 734167),
        ("sintel", "testdata/raw/Sintel.2010.720p.mkv", "testdata/raw/sintel_2010.en.srt", 888032),
    ],
}
THRESHOLDS = [0.008, 0.0125, 0.02, 0.03]
MIN_SILENCE_MS = [300, 500, 800, 1200]


def hmean(values):
    return 0.0 if any(v <= 0 for v in values) else len(values) / sum(1 / v for v in values)


def run(binary, film, extra):
    _, media, srt, total = film
    with tempfile.NamedTemporaryFile(suffix=".json") as out:
        cmd = [binary, "validate", media, "--subtitles", srt, "--total-ms", str(total), "--output", out.name, *extra]
        subprocess.run(cmd, check=True, capture_output=True)
        m = json.load(open(out.name))
    return {
        "speech_recall": m["speech_time_recall"],
        "speech_precision": m["speech_time_precision"],
        "nv_precision": m["non_voice_time_precision"],
        "nv_recall": m["non_voice_time_recall"],
        "score": hmean([m["speech_time_recall"], m["non_voice_time_precision"], m["non_voice_time_recall"]]),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="target/debug/timeline")
    ap.add_argument("--out", default="sweep.json")
    args = ap.parse_args()
    candidates = {"energy t%.4f s%d" % (t, s): ["--vad-engine", "energy", "--threshold", str(t), "--min-silence-ms", str(s)]
                  for t, s in itertools.product(THRESHOLDS, MIN_SILENCE_MS)}
    candidates["shipped (default+silero)"] = ["--vad-engine", "silero"]
    results = {}
    for name, extra in candidates.items():
        results[name] = {role: {f[0]: run(args.binary, f, extra) for f in films} for role, films in FILMS.items()}
        print(name, flush=True)
    summary = {}
    for name, r in results.items():
        summary[name] = {
            "dev": r["dev"]["elephants_dream"]["score"],
            "holdout": sum(v["score"] for v in r["holdout"].values()) / len(r["holdout"]),
        }
    energy = {k: v for k, v in summary.items() if k.startswith("energy")}
    dev_winner = max(energy, key=lambda k: energy[k]["dev"])
    holdout_winner = max(energy, key=lambda k: energy[k]["holdout"])
    rank = sorted(energy, key=lambda k: -energy[k]["holdout"]).index(dev_winner) + 1
    report = {"summary": summary, "dev_winner": dev_winner, "holdout_winner": holdout_winner,
              "dev_winner_holdout_rank": f"{rank}/{len(energy)}", "results": results}
    Path(args.out).write_text(json.dumps(report, indent=2))
    print(json.dumps({k: report[k] for k in ("dev_winner", "holdout_winner", "dev_winner_holdout_rank")}, indent=2))


if __name__ == "__main__":
    main()
