#!/usr/bin/env bash
set -euo pipefail

ffmpeg_available(){
  command -v ffmpeg >/dev/null 2>&1
}

pick_default_input(){
  if ! ffmpeg_available; then
    printf '%s\n' testdata/generated/alternating.wav
    return 0
  fi

  local candidate
  for candidate in \
    testdata/raw/sintel_trailer_2010.mp4 \
    testdata/raw/big_buck_bunny_trailer_2008.mov \
    testdata/raw/elephants_dream_2006.mp4 \
    testdata/raw/elephantsdream_teaser.mp4 \
    testdata/raw/caminandes_gran_dillama.mp4 \
    testdata/raw/eggs_1970.mp4 \
    testdata/raw/windy_day_1967.mp4 \
    testdata/raw/the_hole_1962.mp4 \
    testdata/raw/dinner_time_1928.webm \
    testdata/raw/the_singing_fool_1928.webm
  do
    [[ -f "$candidate" ]] && {
      printf '%s\n' "$candidate"
      return 0
    }
  done
  printf '%s\n' testdata/generated/alternating.wav
}

input=${1:-$(pick_default_input)}
out=${2:-analysis/benchmarks/latest.json}
mkdir -p "$(dirname "$out")"

if [[ "$input" != *.wav ]] && ! ffmpeg_available; then
  echo "ffmpeg unavailable; falling back to deterministic WAV fixture" >&2
  input="testdata/generated/alternating.wav"
fi

if [[ ! -f "$input" && "$input" == "testdata/generated/alternating.wav" ]]; then
  echo "benchmark input missing ($input); generating deterministic fixtures..." >&2
  cargo run --quiet -- gen-fixtures --output-dir testdata/generated
fi

cargo run --quiet -- bench "$input" --output "$out"
echo "benchmark written: $out"
