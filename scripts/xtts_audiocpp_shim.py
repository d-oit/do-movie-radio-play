#!/usr/bin/env python3
"""CPU voice-cloning TTS server speaking the audio.cpp /v1/audio/speech protocol.

Backs the `audio_cpp` provider with Coqui XTTS-v2 so narration is spoken in a
voice cloned from the film's own dialogue (`voice_ref` = base64 WAV or path).
Setup: pip install coqui-tts 'transformers<4.50' fastapi uvicorn soundfile
Run:   python scripts/xtts_audiocpp_shim.py --port 8080
"""
import argparse
import base64
import hashlib
import io
import os
import sys
import tempfile

import soundfile as sf
import torch
import uvicorn
from fastapi import FastAPI, HTTPException
from TTS.api import TTS

os.environ.setdefault("COQUI_TOS_AGREED", "1")
app = FastAPI()
tts = None
LANGS = {"de", "en", "es", "fr", "it", "pt", "pl", "tr", "ru", "nl", "cs", "ar", "zh-cn", "ja", "hu", "ko"}


def speech_only(path: str) -> str:
    """Keep only the speech of a reference clip (<= 10 s).

    Reference clips cut from a film carry music and effects; with them XTTS often
    babbles instead of stopping. Falls back to the original when little speech is found.
    """
    try:
        import numpy as np
        from faster_whisper.audio import decode_audio
        from faster_whisper.vad import VadOptions, get_speech_timestamps

        audio = decode_audio(path, sampling_rate=16000)
        spans = get_speech_timestamps(audio, VadOptions(min_silence_duration_ms=200))
        speech = np.concatenate([audio[s["start"]:s["end"]] for s in spans]) if spans else audio[:0]
        if len(speech) < 3 * 16000:
            return path
        out = path + ".speech.wav"
        sf.write(out, speech[: 10 * 16000], 16000, subtype="PCM_16")
        return out
    except Exception as exc:  # trimming is an optimisation; never fail the request over it
        print(f"warning: reference trimming skipped ({exc})", file=sys.stderr)
        return path


def ref_to_path(ref: str) -> str:
    try:
        raw = base64.b64decode(ref, validate=True)
    except Exception as exc:
        raise HTTPException(400, "voice_ref must be base64 WAV data") from exc
    path = os.path.join(tempfile.gettempdir(), hashlib.sha256(raw).hexdigest()[:16] + ".wav")
    with open(path, "wb") as fh:
        fh.write(raw)
    return speech_only(path)


SAMPLE_RATE = 24000
MAX_ATTEMPTS = 4
SECONDS_PER_CHAR = 0.16  # measured: natural cloned speech runs 0.10-0.14 s per character
SLACK_SECONDS = 1.5


def synthesize_bounded(text: str, speaker_wav: str, language: str, speed: float):
    """XTTS samples, and with some reference clips it often fails to emit
    end-of-speech: one 36-character line took 2.2-27.4 s depending on the seed
    (about 3 s is natural). Retry with a deterministic per-attempt seed and keep
    the first take that fits the text; if none does, return the shortest take.
    Output is reproducible because every seed derives from the text."""
    base = int(hashlib.sha256(text.encode()).hexdigest()[:8], 16)
    limit = int(SAMPLE_RATE * (SLACK_SECONDS + SECONDS_PER_CHAR * len(text)))
    best = None
    for attempt in range(MAX_ATTEMPTS):
        torch.manual_seed(base + attempt)
        wav = tts.tts(text=text, speaker_wav=speaker_wav, language=language, speed=speed)
        if len(wav) <= limit:
            return wav
        if best is None or len(wav) < len(best):
            best = wav
    return best[:limit]


@app.get("/health")
def health():
    return {"status": "ok"}


@app.post("/v1/audio/speech")
def speech(body: dict):
    text = body.get("input", "")
    language = (body.get("language") or "en").lower()[:5]
    language = language if language in LANGS else language[:2]
    if language not in LANGS:
        raise HTTPException(400, f"unsupported language {language}")
    if not body.get("voice_ref"):
        raise HTTPException(400, "voice_ref required: narration must use base64 WAV data")
    wav = synthesize_bounded(text, ref_to_path(body["voice_ref"]), language, float(body.get("speed", 1.0)))
    buf = io.BytesIO()
    sf.write(buf, wav, 24000, format="WAV", subtype="PCM_16")
    return __import__("fastapi").Response(buf.getvalue(), media_type="audio/wav")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8080)
    args = ap.parse_args()
    torch.set_num_threads(os.cpu_count() or 2)
    tts = TTS("tts_models/multilingual/multi-dataset/xtts_v2")
    uvicorn.run(app, host="127.0.0.1", port=args.port)
