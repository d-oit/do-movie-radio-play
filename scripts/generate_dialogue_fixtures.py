#!/usr/bin/env python3
"""Generate synthetic films with real (synthesized) dialogue and exact truth.

The production cue path needs films whose dialogue truth is known second by
second, that are legal to ship, and that were never used to tune a threshold.
Real open films with timed subtitles run out quickly, so this builds scenes
instead: espeak-ng speaks each line, the lines are mixed over music, sound
effects and melodic vocalise, and the truth SRT is written from the same
timeline. Nothing here is derived from the evaluation metrics.

Needs: espeak-ng on PATH. Output is deterministic: same espeak-ng version and
same scene file produce byte-identical audio and truth.

Usage: generate_dialogue_fixtures.py --output-dir testdata/generated/dialogue
"""

import argparse
import json
import math
import shutil
import struct
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

SAMPLE_RATE = 16_000

# Two scenes, deliberately different in language, density and hazard mix.
SCENES = [
    {
        "id": "synthetic_kitchen_en",
        "language": "en",
        "notes": "Two-speaker dialogue with an overlapping line and a short shout buried in a loud impact, so a missed shout is not hidden by a neighbour's pad.",
        "total_ms": 32_000,
        "events": [
            {"kind": "music", "start": 0.0, "end": 9.0, "chord": [220.0, 277.18, 329.63]},
            {"kind": "sfx", "at": 0.6, "dur": 0.35, "seed": 11, "amp": 0.3},
            {"kind": "speech", "at": 1.0, "voice": "en-us", "rate": 165, "amp": 130,
             "text": "Close the door before the neighbours hear us."},
            {"kind": "speech", "at": 3.9, "voice": "en-gb", "rate": 155, "amp": 125,
             "text": "They already know. The whole street knows."},
            {"kind": "speech", "at": 6.2, "voice": "en-us", "rate": 150, "amp": 120,
             "text": "Then we do it tonight, before the tide turns."},
            {"kind": "music", "start": 7.0, "end": 15.0, "chord": [196.0, 246.94, 293.66], "amp": 0.16},
            {"kind": "sfx", "at": 9.9, "dur": 0.6, "seed": 23, "amp": 0.6},
            {"kind": "sfx", "at": 10.4, "dur": 0.6, "seed": 31, "amp": 0.55},
            {"kind": "speech", "at": 10.2, "voice": "en-us", "rate": 200, "amp": 105,
             "text": "Run!"},
            {"kind": "speech", "at": 17.5, "voice": "en-us", "rate": 145, "amp": 60,
             "text": "Do you hear that? Someone is in the hallway."},
            {"kind": "speech", "at": 20.4, "voice": "en-gb", "rate": 150, "amp": 120,
             "text": "It is only the pipes. Go back to sleep."},
            {"kind": "vocalise", "at": 23.5, "dur": 5.5, "freq": 196.0},
            {"kind": "speech", "at": 29.6, "voice": "en-us", "rate": 155, "amp": 125,
             "text": "Tomorrow we start again."},
        ],
    },
    {
        "id": "synthetic_night_de",
        "language": "de",
        "notes": "German dialogue under a continuous score, long pauses, a shout covered by a loud passing effect, and a melodic passage Whisper may mistake for speech.",
        "total_ms": 30_000,
        "events": [
            {"kind": "music", "start": 0.0, "end": 30.0, "chord": [174.61, 220.0, 261.63], "amp": 0.1},
            {"kind": "speech", "at": 1.2, "voice": "de", "rate": 150, "amp": 135,
             "text": "Der Zug faehrt in zehn Minuten."},
            {"kind": "speech", "at": 3.4, "voice": "de", "rate": 145, "amp": 130,
             "text": "Dann muessen wir uns beeilen."},
            {"kind": "speech", "at": 5.6, "voice": "de", "rate": 145, "amp": 130,
             "text": "Nimm den Koffer und bleib dicht hinter mir."},
            {"kind": "sfx", "at": 11.4, "dur": 0.9, "seed": 41, "amp": 0.6},
            {"kind": "sfx", "at": 12.0, "dur": 0.7, "seed": 47, "amp": 0.55},
            {"kind": "speech", "at": 11.8, "voice": "de", "rate": 195, "amp": 100,
             "text": "Halt!"},
            {"kind": "vocalise", "at": 15.5, "dur": 4.0, "freq": 174.61},
            {"kind": "speech", "at": 21.0, "voice": "de", "rate": 140, "amp": 55,
             "text": "Ich habe Angst."},
            {"kind": "speech", "at": 23.4, "voice": "de", "rate": 145, "amp": 130,
             "text": "Wir schaffen das zusammen."},
            {"kind": "speech", "at": 26.6, "voice": "de", "rate": 150, "amp": 125,
             "text": "Der Morgen kommt bald."},
        ],
    },
]


def synthesize(text: str, voice: str, rate: int, amp: int, out: Path) -> None:
    result = subprocess.run(
        [
            "espeak-ng",
            "-v",
            voice,
            "-s",
            str(rate),
            "-a",
            str(amp),
            "-w",
            str(out),
            text,
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0 or not out.exists():
        raise RuntimeError(f"espeak-ng failed for {voice}: {result.stderr.strip()}")


def read_wav(path: Path) -> tuple[list[float], int]:
    with wave.open(str(path), "rb") as handle:
        channels = handle.getnchannels()
        width = handle.getsampwidth()
        rate = handle.getframerate()
        frames = handle.readframes(handle.getnframes())
    if width != 2:
        raise RuntimeError(f"expected 16-bit audio, got {width * 8}-bit")
    count = len(frames) // 2
    raw = struct.unpack(f"<{count}h", frames)
    if channels > 1:
        raw = raw[::channels]
    return [value / 32768.0 for value in raw], rate


def resample(samples: list[float], src_rate: int, dst_rate: int) -> list[float]:
    if src_rate == dst_rate:
        return samples
    ratio = src_rate / dst_rate
    length = int(len(samples) / ratio)
    out = []
    for index in range(length):
        position = index * ratio
        left = int(position)
        right = min(left + 1, len(samples) - 1)
        frac = position - left
        out.append(samples[left] * (1 - frac) + samples[right] * frac)
    return out


def noise_burst(duration_s: float, seed: int, amp: float) -> list[float]:
    count = int(duration_s * SAMPLE_RATE)
    state = seed
    out = []
    for index in range(count):
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        value = (state / 0x7FFFFFFF) * 2 - 1
        envelope = math.exp(-6.0 * index / max(count, 1))
        out.append(value * envelope * amp)
    return out


def chord_bed(
    duration_s: float, chord: list[float], amp: float
) -> list[float]:
    count = int(duration_s * SAMPLE_RATE)
    fade = min(int(0.8 * SAMPLE_RATE), count // 4 or 1)
    out = []
    for index in range(count):
        t = index / SAMPLE_RATE
        value = sum(math.sin(2 * math.pi * freq * t) for freq in chord) / len(chord)
        envelope = 1.0
        if index < fade:
            envelope = index / fade
        elif index > count - fade:
            envelope = max(0.0, (count - index) / fade)
        out.append(value * amp * envelope)
    return out


def vocalise_bed(duration_s: float, freq: float) -> list[float]:
    count = int(duration_s * SAMPLE_RATE)
    out = []
    for index in range(count):
        t = index / SAMPLE_RATE
        vibrato = 1 + 0.012 * math.sin(2 * math.pi * 5.5 * t)
        value = math.sin(2 * math.pi * freq * vibrato * t)
        value += 0.35 * math.sin(4 * math.pi * freq * vibrato * t)
        envelope = math.sin(math.pi * index / count) ** 2
        out.append(value * 0.12 * envelope)
    return out


def mix_into(master: list[float], clip: list[float], start_s: float) -> None:
    offset = int(start_s * SAMPLE_RATE)
    for index, value in enumerate(clip):
        target = offset + index
        if 0 <= target < len(master):
            master[target] += value


def stamp(seconds: float) -> str:
    ms = int(round(seconds * 1000))
    hours, ms = divmod(ms, 3_600_000)
    minutes, ms = divmod(ms, 60_000)
    secs, ms = divmod(ms, 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{ms:03d}"


def build_scene(scene: dict, output_dir: Path, espeak_bin: str) -> dict:
    total_s = scene["total_ms"] / 1000
    master = [0.0] * (int(total_s * SAMPLE_RATE) + SAMPLE_RATE // 2)
    truth: list[tuple[float, float]] = []
    with tempfile.TemporaryDirectory(prefix="tts-") as tmp:
        for index, event in enumerate(scene["events"]):
            kind = event["kind"]
            if kind == "speech":
                wav = Path(tmp) / f"line-{index}.wav"
                synthesize(event["text"], event["voice"], event["rate"], event["amp"], wav)
                samples, rate = read_wav(wav)
                samples = resample(samples, rate, SAMPLE_RATE)
                mix_into(master, samples, event["at"])
                duration = len(samples) / SAMPLE_RATE
                truth.append((event["at"], event["at"] + duration))
            elif kind == "music":
                span = event["end"] - event["start"]
                mix_into(
                    master,
                    chord_bed(span, event["chord"], event.get("amp", 0.08)),
                    event["start"],
                )
            elif kind == "sfx":
                mix_into(
                    master,
                    noise_burst(event["dur"], event["seed"], event.get("amp", 0.3)),
                    event["at"],
                )
            elif kind == "vocalise":
                mix_into(master, vocalise_bed(event["dur"], event["freq"]), event["at"])
            else:
                raise RuntimeError(f"{scene['id']}: unknown event kind '{kind}'")

    peak = max((abs(value) for value in master), default=0.0)
    gain = min(1.0, 0.89 / peak) if peak > 0 else 1.0
    pcm = bytes()
    frames = bytearray()
    for value in master[: int(total_s * SAMPLE_RATE)]:
        frames += struct.pack("<h", int(max(-1.0, min(1.0, value * gain)) * 32767))

    wav_path = output_dir / f"{scene['id']}.wav"
    with wave.open(str(wav_path), "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(SAMPLE_RATE)
        handle.writeframes(bytes(frames))

    truth_path = output_dir / f"{scene['id']}.truth.srt"
    lines = []
    for number, (start, end) in enumerate(truth, 1):
        lines.append(f"{number}\n{stamp(start)} --> {stamp(end)}\n[speech]\n")
    truth_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    return {
        "id": scene["id"],
        "language": scene["language"],
        "notes": scene["notes"],
        "media": str(wav_path),
        "truth": str(truth_path),
        "total_ms": scene["total_ms"],
        "speech_events": len(truth),
        "generator": "espeak-ng",
        "espeak_version": espeak_bin,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", default="testdata/generated/dialogue")
    parser.add_argument(
        "--manifest", default=None, help="write a generation report here"
    )
    args = parser.parse_args()

    binary = shutil.which("espeak-ng")
    if not binary:
        print(
            "error: espeak-ng is required to build synthetic dialogue fixtures "
            "(apt-get install espeak-ng)",
            file=sys.stderr,
        )
        return 3
    version = subprocess.run(
        ["espeak-ng", "--version"], capture_output=True, text=True
    ).stdout.strip()

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    scenes = [build_scene(scene, output_dir, version) for scene in SCENES]

    report = {"version": 1, "generator": version, "scenes": scenes}
    manifest_path = Path(args.manifest) if args.manifest else output_dir / "manifest.json"
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    for scene in scenes:
        print(
            f"{scene['id']}: {scene['speech_events']} speech events, "
            f"{scene['total_ms'] / 1000:.0f}s, language {scene['language']}"
        )
    print(f"wrote fixtures to {output_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
