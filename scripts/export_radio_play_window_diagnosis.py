#!/usr/bin/env python3
"""Export per-window false-positive/false-negative examples for one holdout.

The readiness report says *that* a film regressed; this tool says *where*. It
compares a predicted timeline against truth and emits contiguous windows with
their time range, predicted label, predicted acoustic tags and ground-truth
label, so a failure can be inspected without re-running the whole sweep.

Truth is either an SRT file (speech cues) or a timeline JSON. Predicted input is
either an existing timeline JSON or a media file, which is extracted with the
`timeline` CLI using the same config/engine as the sweep.
"""

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

TIMESTAMP = re.compile(
    r"(\d\d):(\d\d):(\d\d)[,.](\d+)\s*-->\s*(\d\d):(\d\d):(\d\d)[,.](\d+)"
)


def parse_srt_entries(path: Path) -> list[tuple[int, int, str]]:
    text = path.read_text(encoding="utf-8-sig", errors="ignore")
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    entries = []
    for match in TIMESTAMP.finditer(text):
        h1, m1, s1, f1, h2, m2, s2, f2 = match.groups()
        start = (int(h1) * 3600 + int(m1) * 60 + int(s1)) * 1000 + int(f1)
        end = (int(h2) * 3600 + int(m2) * 60 + int(s2)) * 1000 + int(f2)
        if end > start:
            body = text[match.end() :].split("\n\n", 1)[0].strip()
            entries.append((start, end, body))
    return entries


def merge(spans: list[tuple[int, int]]) -> list[tuple[int, int]]:
    out: list[tuple[int, int]] = []
    for start, end in sorted(spans):
        if end <= start:
            continue
        if out and start <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], end))
        else:
            out.append((start, end))
    return out


def complement(spans: list[tuple[int, int]], total_ms: int) -> list[tuple[int, int]]:
    out: list[tuple[int, int]] = []
    cursor = 0
    for start, end in merge(spans):
        if start > cursor:
            out.append((cursor, min(start, total_ms)))
        cursor = max(cursor, end)
        if cursor >= total_ms:
            break
    if cursor < total_ms:
        out.append((cursor, total_ms))
    return out


def load_timeline(path: Path) -> dict:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload.get("segments"), list):
        raise ValueError(f"{path}: no 'segments' list")
    return payload


def segments_of(timeline: dict, kind: str) -> list[dict]:
    return [seg for seg in timeline["segments"] if seg.get("kind") == kind]


def spans_of(segments: list[dict]) -> list[tuple[int, int]]:
    return merge(
        [
            (int(seg["start_ms"]), int(seg["end_ms"]))
            for seg in segments
            if int(seg.get("end_ms", 0)) > int(seg.get("start_ms", 0))
        ]
    )


def tags_at(segments: list[dict], start_ms: int, end_ms: int) -> list[str]:
    tags: list[str] = []
    for seg in segments:
        if int(seg["end_ms"]) <= start_ms or int(seg["start_ms"]) >= end_ms:
            continue
        for tag in seg.get("tags") or []:
            if tag not in tags:
                tags.append(tag)
    return tags


def label_at(spans: list[tuple[int, int]], point_ms: int) -> bool:
    return any(start <= point_ms < end for start, end in spans)


def truth_text_at(entries: list[tuple[int, int, str]], start_ms: int, end_ms: int) -> str:
    texts = [
        body
        for cue_start, cue_end, body in entries
        if cue_end > start_ms and cue_start < end_ms and body
    ]
    return " ".join(texts)[:300]


def extract_timeline(
    media: Path,
    config: Path | None,
    vad_engine: str | None,
    out: Path,
    tag: bool = True,
) -> dict:
    cmd = ["cargo", "run", "--quiet", "--", "extract", str(media), "--output", str(out)]
    if config is not None:
        cmd.extend(["--config", str(config)])
    if vad_engine:
        cmd.extend(["--vad-engine", vad_engine])
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(
            f"extract failed (exit={result.returncode})\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    if not tag:
        return load_timeline(out)
    tagged = out.with_suffix(".tagged.json")
    tag_cmd = [
        "cargo",
        "run",
        "--quiet",
        "--",
        "tag",
        str(media),
        "--input",
        str(out),
        "--output",
        str(tagged),
    ]
    tag_result = subprocess.run(tag_cmd, capture_output=True, text=True)
    if tag_result.returncode != 0:
        # Tags are diagnostic context, not a gate: keep the untagged timeline.
        print(
            f"WARN: tagging failed for {media}; diagnosis will have no tags\n"
            f"{tag_result.stderr}",
            file=sys.stderr,
        )
        return load_timeline(out)
    return load_timeline(tagged)


def build_examples(
    predicted_non_voice: list[dict],
    predicted_speech: list[dict],
    truth_speech: list[tuple[int, int]],
    truth_entries: list[tuple[int, int, str]],
    total_ms: int,
    window_ms: int,
) -> list[dict]:
    pred_nv_spans = spans_of(predicted_non_voice)
    pred_speech_spans = spans_of(predicted_speech)
    truth_nv_spans = complement(truth_speech, total_ms)

    examples: list[dict] = []
    current: dict | None = None
    for start in range(0, total_ms, window_ms):
        end = min(start + window_ms, total_ms)
        midpoint = (start + end) // 2
        predicted_nv = label_at(pred_nv_spans, midpoint)
        truth_speech_here = label_at(truth_speech, midpoint)
        truth_nv = label_at(truth_nv_spans, midpoint)

        kind = None
        if predicted_nv and truth_speech_here:
            kind = "false_positive"
        elif not predicted_nv and truth_nv:
            kind = "false_negative"

        if kind is None:
            current = None
            continue
        if current and current["kind"] == kind and current["end_ms"] == start:
            current["end_ms"] = end
            continue
        current = {
            "kind": kind,
            "start_ms": start,
            "end_ms": end,
            "predicted_label": "non_voice" if predicted_nv else "speech",
            "truth_label": "speech" if truth_speech_here else "non_voice",
            "predicted_tags": tags_at(predicted_non_voice, start, end),
            "truth_text": truth_text_at(truth_entries, start, end),
        }
        examples.append(current)

    for example in examples:
        example["duration_ms"] = example["end_ms"] - example["start_ms"]
    return examples


def diagnose(
    entry_id: str,
    timeline_path: Path | None,
    media: Path | None,
    config: Path | None,
    vad_engine: str | None,
    truth_path: Path,
    total_ms: int | None,
    window_ms: int,
    max_examples: int,
    output_json: Path,
    output_md: Path,
    tag: bool = True,
) -> dict:
    if not truth_path.exists():
        raise FileNotFoundError(f"missing truth: {truth_path}")

    if timeline_path is not None:
        timeline = load_timeline(timeline_path)
    elif media is not None:
        if not media.exists():
            raise FileNotFoundError(f"missing media: {media}")
        # Intermediates are scratch data; only the diagnosis is an artifact.
        with tempfile.TemporaryDirectory() as tmp:
            timeline = extract_timeline(
                media,
                config,
                vad_engine,
                Path(tmp) / "timeline.json",
                tag=tag,
            )
    else:
        raise ValueError("provide --timeline or --media")

    if truth_path.suffix.lower() == ".json":
        truth_timeline = load_timeline(truth_path)
        truth_entries = [
            (int(seg["start_ms"]), int(seg["end_ms"]), "")
            for seg in segments_of(truth_timeline, "speech")
        ]
        truth_speech = spans_of(segments_of(truth_timeline, "speech"))
        total_ms = total_ms or max(
            (int(seg["end_ms"]) for seg in truth_timeline["segments"]), default=0
        )
    else:
        if not total_ms:
            raise ValueError("--total-ms is required for SRT truth")
        truth_entries = parse_srt_entries(truth_path)
        truth_speech = merge([(start, end) for start, end, _ in truth_entries])

    if total_ms <= 0:
        raise ValueError("total_ms must be positive")

    predicted_non_voice = segments_of(timeline, "non_voice")
    predicted_speech = segments_of(timeline, "speech")
    examples = build_examples(
        predicted_non_voice,
        predicted_speech,
        truth_speech,
        truth_entries,
        total_ms,
        window_ms,
    )

    fp_ms = sum(e["duration_ms"] for e in examples if e["kind"] == "false_positive")
    fn_ms = sum(e["duration_ms"] for e in examples if e["kind"] == "false_negative")
    report = {
        "entry_id": entry_id,
        "timeline": str(timeline_path or media),
        "truth": str(truth_path),
        "total_ms": total_ms,
        "window_ms": window_ms,
        "false_positive_ms": fp_ms,
        "false_negative_ms": fn_ms,
        "example_count": len(examples),
        "examples": examples[:max_examples],
    }

    output_json.parent.mkdir(parents=True, exist_ok=True)
    output_json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    lines = [
        f"# Window Diagnosis: {entry_id}",
        "",
        f"- Truth: `{truth_path}`",
        f"- Window: {window_ms} ms; total {total_ms} ms",
        f"- False-positive time: {fp_ms} ms; false-negative time: {fn_ms} ms",
        f"- Examples: {len(examples)} (showing {min(len(examples), max_examples)})",
        "",
        "| Kind | Start | End | Predicted | Truth | Tags | Truth text |",
        "|---|---:|---:|---|---|---|---|",
    ]
    for example in examples[:max_examples]:
        tags = ",".join(example["predicted_tags"]) or "-"
        text = example["truth_text"].replace("|", "/") or "-"
        lines.append(
            f"| {example['kind']} | {example['start_ms']} | {example['end_ms']} | "
            f"{example['predicted_label']} | {example['truth_label']} | {tags} | {text} |"
        )

    output_md.parent.mkdir(parents=True, exist_ok=True)
    output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print(f"wrote diagnosis json: {output_json}")
    print(f"wrote diagnosis markdown: {output_md}")
    return report


def batch_from_readiness(
    readiness_path: Path,
    manifest_path: Path,
    output_dir: Path,
    window_ms: int,
    max_examples: int,
    tag: bool = True,
) -> int:
    readiness = json.loads(readiness_path.read_text(encoding="utf-8"))
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    entries = {entry.get("id"): entry for entry in manifest.get("entries", [])}
    failing = readiness.get("failing_entries", [])
    if not failing:
        print("no failing entries to diagnose")
        return 0
    output_dir.mkdir(parents=True, exist_ok=True)
    for row in failing:
        entry_id = row.get("id")
        entry = entries.get(entry_id)
        if entry is None:
            print(f"WARN: {entry_id} not in manifest; skipping", file=sys.stderr)
            continue
        diagnose(
            entry_id=entry_id,
            timeline_path=None,
            media=Path(entry["input_media"]),
            config=Path(entry["config_path"]) if entry.get("config_path") else None,
            vad_engine=entry.get("vad_engine"),
            truth_path=Path(entry["truth_path"]),
            total_ms=entry.get("total_ms"),
            window_ms=window_ms,
            max_examples=max_examples,
            output_json=output_dir / f"{entry_id}.json",
            output_md=output_dir / f"{entry_id}.md",
            tag=tag,
        )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Export per-window FP/FN diagnosis for a radio-play holdout"
    )
    parser.add_argument("--entry-id")
    parser.add_argument("--timeline", help="Predicted timeline JSON")
    parser.add_argument("--media", help="Media file to extract when --timeline is absent")
    parser.add_argument("--config", help="Analysis config for extraction")
    parser.add_argument("--vad-engine", help="VAD engine for extraction")
    parser.add_argument("--truth", help="Truth SRT or timeline JSON")
    parser.add_argument("--total-ms", type=int, help="Media length (required for SRT truth)")
    parser.add_argument("--window-ms", type=int, default=100)
    parser.add_argument("--max-examples", type=int, default=50)
    parser.add_argument(
        "--no-tag",
        action="store_true",
        help="Skip acoustic tagging of the extracted timeline",
    )
    parser.add_argument(
        "--readiness-report",
        help="Diagnose every failing entry in this readiness report (batch mode)",
    )
    parser.add_argument(
        "--manifest",
        default="testdata/validation/radio-play-manifest.json",
        help="Manifest used by batch mode",
    )
    parser.add_argument(
        "--output-dir",
        default="analysis/validation/window-diagnosis",
        help="Batch mode output directory",
    )
    parser.add_argument(
        "--output-json",
        default="analysis/validation/radio-play-window-diagnosis.json",
    )
    parser.add_argument(
        "--output-md",
        default="analysis/learnings/latest-radio-play-window-diagnosis.md",
    )
    args = parser.parse_args()

    if args.readiness_report:
        return batch_from_readiness(
            Path(args.readiness_report),
            Path(args.manifest),
            Path(args.output_dir),
            args.window_ms,
            args.max_examples,
            tag=not args.no_tag,
        )

    if not args.entry_id or not args.truth:
        parser.error("--entry-id and --truth are required outside batch mode")
    diagnose(
        entry_id=args.entry_id,
        timeline_path=Path(args.timeline) if args.timeline else None,
        media=Path(args.media) if args.media else None,
        config=Path(args.config) if args.config else None,
        vad_engine=args.vad_engine,
        truth_path=Path(args.truth),
        total_ms=args.total_ms,
        window_ms=args.window_ms,
        max_examples=args.max_examples,
        output_json=Path(args.output_json),
        output_md=Path(args.output_md),
        tag=not args.no_tag,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
