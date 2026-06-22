import modal
import logging

logger = logging.getLogger(__name__)

app = modal.App("do-movie-radio-play-tts-piper")

image = (
    modal.Image.debian_slim(python_version="3.11")
    .pip_install(
        "piper-tts>=1.4.0",
        "fastapi[standard]",
    )
    .run_commands(
        "apt-get update && apt-get install -y wget libsndfile1",
        "mkdir -p /models",
        "wget -O /models/de_DE-thorsten-high.onnx https://huggingface.co/rhasspy/piper-voices/resolve/main/de/de_DE/thorsten/high/de_DE-thorsten-high.onnx",
        "wget -O /models/de_DE-thorsten-high.onnx.json https://huggingface.co/rhasspy/piper-voices/resolve/main/de/de_DE/thorsten/high/de_DE-thorsten-high.onnx.json",
    )
)

@app.function(
    image=image,
    scaledown_window=60,
)
@modal.fastapi_endpoint(method="POST")
def generate_speech(text: str, language: str = "de"):
    import io
    import wave
    from fastapi.responses import Response

    try:
        from piper import PiperVoice

        logger.info("Loading Piper voice model...")
        voice = PiperVoice.load(
            "/models/de_DE-thorsten-high.onnx",
            config_path="/models/de_DE-thorsten-high.onnx.json",
        )
        logger.info("Piper voice loaded successfully")

        wav_buffer = io.BytesIO()
        with wave.open(wav_buffer, "wb") as wav_file:
            voice.synthesize_wav(text, wav_file)

        wav_bytes = wav_buffer.getvalue()
        logger.info(f"Synthesized {len(wav_bytes)} bytes for text: {text[:50]}...")
        return Response(content=wav_bytes, media_type="audio/wav")

    except Exception as e:
        logger.error(f"TTS error: {type(e).__name__}: {e}")
        raise

if __name__ == "__main__":
    app.deploy("do-movie-radio-play-tts-piper")
