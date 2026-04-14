#!/usr/bin/env bash
set -euo pipefail
mkdir -p testdata/raw
fetch(){
  local url="$1"; local out="$2"
  if [[ -s "$out" ]]; then echo "ok $out"; return 0; fi
  curl -L --fail --retry 3 "$url" -o "$out"
  [[ -s "$out" ]] || { echo "empty: $out"; exit 1; }
}
# Layer 3: Post-2000 video fixtures only.
# Elephants Dream (2006) - Blender Open Movie
fetch "https://archive.org/download/ElephantsDream/ed_1024_512kb.mp4" "testdata/raw/elephants_dream_2006.mp4"
# Multilingual subtitle fixtures for non-English validation coverage
fetch "https://commons.wikimedia.org/wiki/TimedText:Elephants_Dream_(2006).webm.es.srt?action=raw" "testdata/raw/elephants_dream_2006.es.srt"
fetch "https://commons.wikimedia.org/wiki/TimedText:Elephants_Dream_(2006).webm.de.srt?action=raw" "testdata/raw/elephants_dream_2006.de.srt"
# Big Buck Bunny trailer (2008) - Blender Open Movie trailer
fetch "https://download.blender.org/peach/trailer/trailer_480p.mov" "testdata/raw/big_buck_bunny_trailer_2008.mov"
# Sintel trailer (2010) - Blender Open Movie trailer
fetch "https://download.blender.org/durian/trailer/sintel_trailer-720p.mp4" "testdata/raw/sintel_trailer_2010.mp4"

for f in testdata/raw/elephants_dream_2006.mp4 testdata/raw/elephants_dream_2006.es.srt testdata/raw/elephants_dream_2006.de.srt testdata/raw/big_buck_bunny_trailer_2008.mov testdata/raw/sintel_trailer_2010.mp4; do
  [[ -s "$f" ]] || { echo "missing $f"; exit 1; }
done
echo "assets ready in testdata/raw"
