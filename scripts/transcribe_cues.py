#!/usr/bin/env python3
"""Write timed dialogue cues (SRT) for a film that has no subtitles.

Speech-to-text over the film's own audio gives exact dialogue spans that
`radio-play --subtitles` uses as a hard veto, so narration never lands on speech.
Needs: pip install faster-whisper. Text is discarded by the pipeline; only the
timings matter, so a small model is enough.

Usage: transcribe_cues.py MEDIA OUT.srt [--model base] [--language de] [--vad]
"""
import argparse
import sys

from faster_whisper import WhisperModel


def stamp(seconds: float) -> str:
    ms = int(round(seconds * 1000))
    h, ms = divmod(ms, 3_600_000)
    m, ms = divmod(ms, 60_000)
    s, ms = divmod(ms, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("media")
    ap.add_argument("out")
    ap.add_argument("--model", default="base")
    ap.add_argument("--language", default=None)
    ap.add_argument("--vad", action=argparse.BooleanOptionalAction, default=False,
                    help="whisper's VAD pre-filter: ~6x faster but drops short shouts over loud action "
                    "(measured: dialogue leak 6.4%% vs 3.1%% on Tears of Steel)")
    ap.add_argument("--vad-threshold", type=float, default=0.15)
    args = ap.parse_args()
    model = WhisperModel(args.model, device="cpu", compute_type="int8")
    segments, _ = model.transcribe(
        args.media,
        language=args.language,
        vad_filter=args.vad,
        vad_parameters={"min_silence_duration_ms": 400, "threshold": args.vad_threshold},
        word_timestamps=True,
        condition_on_previous_text=False,
    )
    cues = []
    for seg in segments:
        words = [w for w in (seg.words or []) if w.end > w.start]
        if not words:
            continue
        start, end = words[0].start, words[-1].end
        # A long segment may hide a pause; split where words are > 1 s apart.
        cur = [words[0]]
        for prev, w in zip(words, words[1:]):
            if w.start - prev.end > 1.0:
                cues.append((cur[0].start, cur[-1].end))
                cur = []
            cur.append(w)
        cues.append((cur[0].start, cur[-1].end))
    with open(args.out, "w", encoding="utf-8") as fh:
        for i, (a, b) in enumerate(cues, 1):
            fh.write(f"{i}\n{stamp(a)} --> {stamp(b)}\n[speech]\n\n")
    print(f"wrote {len(cues)} cues to {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
