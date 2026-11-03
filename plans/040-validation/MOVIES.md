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

Note: `sintel_trailer_2010.mp4` is the preferred input for `cargo bench --bench pipeline_bench` (via `preferred_media_path()`), which runs in the `Benchmarks` CI job. That helper falls back through Big Buck Bunny, Elephants Dream and finally synthetic audio, so the bench degrades rather than failing — but the abort in `fetch_test_assets.sh` fires first and kills the whole job, before the fallback can be used. No Rust test references `testdata/raw/`, so the `Test` and `Code Coverage` jobs need none of these trailers. Its subtitle truth (`sintel_trailer_2010.srt`) has never existed, so "srt validation" in the table above refers only to the manual commands below. See `plans/FOLLOWUPS.md`.

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
