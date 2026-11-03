# Layer 3 - Movies + Subtitles (Realism)

## Policy

- Prefer post-2000 fixtures for smoke, validation, and benchmark runs.
- Keep legacy pre-2000 fixtures only as fallback compatibility when they already exist locally.
- Keep deterministic generated-audio fallback when no movie fixture is available.

## Primary Post-2000 Fixtures

| File | Year | Source | Mirror | Intended Use |
|------|------|--------|--------|--------------|
| `testdata/raw/elephants_dream_2006.mp4` | 2006 | Archive.org | download.blender.org (fallback) | decode/bench smoke |
| `testdata/raw/big_buck_bunny_trailer_2008.mov` | 2008 | download.blender.org | [Wikimedia Commons 400p ogv](https://upload.wikimedia.org/wikipedia/commons/b/b3/Big_Buck_Bunny_Trailer_400p.ogv) | decode/bench smoke |
| `testdata/raw/sintel_trailer_2010.mp4` | 2010 | download.blender.org | [Wikimedia Commons 1080p ogv](https://upload.wikimedia.org/wikipedia/commons/0/06/Sintel_trailer-1080p.ogv) | decode/bench |

Mirrors were added on 2026-11-03 after a `download.blender.org` outage aborted every CI job that fetches assets. `fetch_video_with_mirrors` tries the primary host first and falls back to the mirror, normalising it to 720p H.264/AAC in the target container so decode timings stay comparable. Verified: the Sintel mirror is the same 52 s cut (52.25 s vs 52.21 s) and decodes through `timeline extract` (2601 frames).

Note: `sintel_trailer_2010.mp4` is listed as a critical asset, but no CI job consumes it — the coverage/test jobs only need generated fixtures, and the benchmark job benchmarks `elephants_dream_2006.mp4`. Its subtitle truth (`sintel_trailer_2010.srt`) has never existed, so "srt validation" in the table above refers only to the manual commands below. See `plans/FOLLOWUPS.md`.

## Non-English Subtitle Fixtures

| File | Language | Source |
|------|----------|--------|
| `testdata/raw/elephants_dream_2006.es.srt` | Spanish | Wikimedia TimedText |
| `testdata/raw/elephants_dream_2006.de.srt` | German | Wikimedia TimedText |

## Fallback Fixtures (Legacy)

Legacy fixtures such as `the_hole_1962.mp4`, `windy_day_1967.mp4`, `eggs_1970.mp4`, and older webm assets may still be used as fallback by test selection logic to avoid breaking existing local setups.

## Validation Example

```bash
timeline validate testdata/raw/sintel_trailer_2010.mp4 \
  --subtitles testdata/raw/sintel_trailer_2010.srt \
  --total-ms 53000 \
  --profile movie \
  --output analysis/validation/sintel_trailer_2010.json
```

## Rationale

- Better match to modern decode pipelines and contemporary audio characteristics.
- More stable benchmark interpretation than very old low-fidelity film audio.
- Backward compatibility retained during migration via fallback ordering.
