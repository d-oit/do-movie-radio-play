# Layer 3 - Movies + Subtitles (Realism)

## Policy

- Prefer post-2000 fixtures for smoke, validation, and benchmark runs.
- Keep legacy pre-2000 fixtures only as fallback compatibility when they already exist locally.
- Keep deterministic generated-audio fallback when no movie fixture is available.

## Primary Post-2000 Fixtures

| File | Year | Source | Intended Use |
|------|------|--------|--------------|
| `testdata/raw/elephants_dream_2006.mp4` | 2006 | Blender/Open Movie (Archive.org mirror) | decode/bench smoke |
| `testdata/raw/big_buck_bunny_trailer_2008.mov` | 2008 | Blender trailer | decode/bench smoke |
| `testdata/raw/sintel_trailer_2010.mp4` | 2010 | Blender trailer | decode/bench/srt validation |

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
