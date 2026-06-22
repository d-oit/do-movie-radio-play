import modal
import os

app = modal.App("do-movie-radio-play-tts")

# Image for Coqui XTTS v2
image = modal.Image.debian_slim(python_version="3.11").pip_install(
    "TTS==0.22.0",
    "fastapi[standard]",
    "numpy<2.0.0",
)

@app.function(
    gpu="T4",
    image=image,
    scaledown_window=60,
    container_idle_timeout=60,
)
@modal.fastapi_endpoint(method="POST")
def generate_speech(text: str, speaker_wav: str = None, language: str = "de"):
    from TTS.api import TTS
    import tempfile
    import io

    # Load model (cached in container)
    tts = TTS("tts_models/multilingual/multi-dataset/xtts_v2", gpu=True)

    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        # If speaker_wav is provided, it's used for voice cloning
        # In this example, we assume a default speaker if none provided
        tts.tts_to_file(
            text=text,
            file_path=f.name,
            speaker_wav=speaker_wav,
            language=language
        )

        with open(f.name, "rb") as wav_file:
            return wav_file.read()

if __name__ == "__main__":
    app.deploy("do-movie-radio-play-tts")
