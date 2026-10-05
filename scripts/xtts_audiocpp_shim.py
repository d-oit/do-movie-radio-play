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


def ref_to_path(ref: str) -> str:
    if ref and os.path.isfile(ref):
        return ref
    try:
        raw = base64.b64decode(ref, validate=True)
    except Exception as exc:
        raise HTTPException(400, "voice_ref must be a file path or base64 WAV") from exc
    path = os.path.join(tempfile.gettempdir(), hashlib.sha256(raw).hexdigest()[:16] + ".wav")
    with open(path, "wb") as fh:
        fh.write(raw)
    return path


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
        raise HTTPException(400, "voice_ref required: narration must use a film voice")
    wav = tts.tts(
        text=text,
        speaker_wav=ref_to_path(body["voice_ref"]),
        language=language,
        speed=float(body.get("speed", 1.0)),
    )
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
