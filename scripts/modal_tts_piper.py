import modal

app = modal.App("do-movie-radio-play-tts-piper")

# Image for Piper TTS (CPU optimized)
image = (
    modal.Image.debian_slim(python_version="3.11")
    .pip_install(
        "piper-tts==1.2.0",
        "fastapi[standard]",
    )
    .run_commands(
        "apt-get update && apt-get install -y wget",
        "mkdir -p /models",
        "wget -O /models/de_DE-thorsten-high.onnx https://github.com/rhasspy/piper/releases/download/v1.0.0/voice-de_DE-thorsten-high.onnx",
        "wget -O /models/de_DE-thorsten-high.onnx.json https://github.com/rhasspy/piper/releases/download/v1.0.0/voice-de_DE-thorsten-high.onnx.json",
    )
)

@app.function(
    image=image,
    scaledown_window=60,
    container_idle_timeout=60,
)
@modal.fastapi_endpoint(method="POST")
def generate_speech(text: str, language: str = "de"):
    from piper.voice import PiperVoice
    import tempfile
    import wave

    voice = PiperVoice.load("/models/de_DE-thorsten-high.onnx", config_path="/models/de_DE-thorsten-high.onnx.json")

    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        with wave.open(f.name, "wb") as wav_file:
            voice.synthesize(text, wav_file)

        with open(f.name, "rb") as result_file:
            return result_file.read()

if __name__ == "__main__":
    app.deploy("do-movie-radio-play-tts-piper")
