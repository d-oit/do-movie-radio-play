#!/usr/bin/env python3
"""Write timed dialogue cues (SRT) for a film that has no subtitles.

Speech-to-text over the film's own audio gives exact dialogue spans that
`radio-play --subtitles` uses as a hard veto, so narration never lands on speech.
Needs: pip install faster-whisper. Text is discarded by the pipeline; only the
timings matter, so a small model is enough.

Usage: transcribe_cues.py MEDIA OUT.srt [--model base] [--language de] [--vad]

Decoding is fixed (greedy, beam 5, no previous-text conditioning) so repeated
runs on the same media and model revision produce byte-identical cues. The
model revision and cache directory are recorded in a sidecar `.meta.json` so a
report can name the exact weights that produced it.
"""
import argparse
import hashlib
import json
import sys

try:
    from faster_whisper import WhisperModel
except ImportError as exc:  # explicit degradation, not a traceback
    print(
        "error: faster-whisper is not installed; install it with "
        "'pip install faster-whisper' or run radio-play with --no-auto-cues "
        f"to use VAD-only narration ({exc})",
        file=sys.stderr,
    )
    raise SystemExit(3)

# Pinned Systran/faster-whisper-base revision; bump deliberately with a report.
DEFAULT_MODEL_REVISION = "ebe41f70d5b6dfa9166e2c581c45c9c0cfc57b66"


def stamp(seconds: float) -> str:
    ms = int(round(seconds * 1000))
    h, ms = divmod(ms, 3_600_000)
    m, ms = divmod(ms, 60_000)
    s, ms = divmod(ms, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


def gate_by_vad_evidence(media: str, cues: list, floor: float) -> list:
    """Drop cues the bundled Silero model is sure contain no speech.

    Whisper (no VAD pre-filter, so short shouts survive) also hallucinates speech
    over score and ambience; on Sintel only 14% of its cue time was real dialogue.
    A cue is kept when Silero's peak speech probability inside it reaches `floor`.
    Measured over three films: floor 0.1 keeps ~37% more narratable time than
    ungated cues for +0.6 pp dialogue leak, and the result is flat for 0.1-0.3.
    """
    if floor <= 0 or not cues:
        return cues
    try:
        import numpy as np
        from faster_whisper.audio import decode_audio
        from faster_whisper.vad import get_vad_model

        audio = decode_audio(media, sampling_rate=16000)
        window = 512
        probs = np.asarray(get_vad_model()(audio[: len(audio) // window * window], num_samples=window)).reshape(-1)
    except Exception as exc:  # gating is an optimisation; never lose all cues over it
        print(f"warning: VAD evidence gate unavailable ({exc}); keeping all cues", file=sys.stderr)
        return cues
    step = window / 16000
    kept = []
    for start, end in cues:
        lo = int(start / step)
        if probs[lo : max(int(end / step), lo + 1)].max(initial=0.0) >= floor:
            kept.append((start, end))
    print(f"vad evidence gate kept {len(kept)}/{len(cues)} cues", file=sys.stderr)
    return kept


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("media")
    ap.add_argument("out")
    ap.add_argument("--model", default="base")
    ap.add_argument("--model-revision", default=DEFAULT_MODEL_REVISION,
                    help="Hugging Face revision to pin (default: the pinned base revision)")
    ap.add_argument("--cache-dir", default=None,
                    help="model download cache; recorded in the metadata sidecar")
    ap.add_argument("--language", default=None)
    ap.add_argument("--vad", action=argparse.BooleanOptionalAction, default=False,
                    help="whisper's VAD pre-filter: ~6x faster but drops short shouts over loud action "
                    "(measured: dialogue leak 6.4%% vs 3.1%% on Tears of Steel)")
    ap.add_argument("--vad-threshold", type=float, default=0.15)
    ap.add_argument("--min-vad-evidence", type=float, default=0.1,
                    help="drop cues whose peak Silero speech probability is below this (0 disables)")
    ap.add_argument("--meta-out", default=None,
                    help="write decode/model identity metadata here (default: OUT.meta.json)")
    args = ap.parse_args()
    model = WhisperModel(
        args.model,
        device="cpu",
        compute_type="int8",
        revision=args.model_revision,
        download_root=args.cache_dir,
    )
    segments, _ = model.transcribe(
        args.media,
        language=args.language,
        vad_filter=args.vad,
        vad_parameters={"min_silence_duration_ms": 400, "threshold": args.vad_threshold},
        word_timestamps=True,
        condition_on_previous_text=False,
        # No sampling fallback: it made cues differ run to run (70-99 cues on one film).
        temperature=0.0,
        beam_size=5,
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
    cues = gate_by_vad_evidence(args.media, cues, args.min_vad_evidence)
    with open(args.out, "w", encoding="utf-8") as fh:
        for i, (a, b) in enumerate(cues, 1):
            fh.write(f"{i}\n{stamp(a)} --> {stamp(b)}\n[speech]\n\n")
    with open(args.out, "rb") as fh:
        digest = hashlib.sha256(fh.read()).hexdigest()
    meta_path = args.meta_out or f"{args.out}.meta.json"
    with open(meta_path, "w", encoding="utf-8") as fh:
        json.dump(
            {
                "model": args.model,
                "model_revision": args.model_revision,
                "cache_dir": args.cache_dir,
                "device": "cpu",
                "compute_type": "int8",
                "decode": {
                    "temperature": 0.0,
                    "beam_size": 5,
                    "condition_on_previous_text": False,
                    "word_timestamps": True,
                    "vad_filter": args.vad,
                    "vad_threshold": args.vad_threshold,
                    "min_vad_evidence": args.min_vad_evidence,
                },
                "cue_count": len(cues),
                "output_sha256": digest,
            },
            fh,
            indent=2,
            sort_keys=True,
        )
        fh.write("\n")
    print(f"wrote {len(cues)} cues to {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
