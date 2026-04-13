#!/usr/bin/env bash
set -euo pipefail
mkdir -p testdata/raw
fetch(){
  local url="$1"; local out="$2"
  if [[ -s "$out" ]]; then echo "ok $out"; return 0; fi
  curl -L --fail --retry 3 "$url" -o "$out"
  [[ -s "$out" ]] || { echo "empty: $out"; exit 1; }
}
# Wikimedia Commons public domain/safe fixture sources.
fetch "https://upload.wikimedia.org/wikipedia/commons/0/0f/Br%C3%BCder_%281929%29.webm" "testdata/raw/bruder-1929.webm"
fetch "https://upload.wikimedia.org/wikipedia/commons/0/0a/CPIDL_German_-_Hallo.ogg" "testdata/raw/cpidl-hallo.ogg"
fetch "https://upload.wikimedia.org/wikipedia/commons/0/0a/De-Bier.ogg" "testdata/raw/de-bier.ogg"

# Layer 3: Movie + subtitle validation (public domain films with VOICE)
# The Singing Fool (1928) - first sound film to reach #1 at box office, has actual dialogue!
fetch "https://upload.wikimedia.org/wikipedia/commons/2/2e/The_Singing_Fool_%281928%29.webm" "testdata/raw/the_singing_fool_1928.webm"
# The Hole (1962) - Academy Award winner with actual dialogue by Dizzy Gillespie
fetch "https://archive.org/download/1960publicdomainanimation/1962%20-%20The%20Hole.ia.mp4" "testdata/raw/the_hole_1962.mp4"
# Windy Day (1967) - experimental animated short
fetch "https://archive.org/download/1960publicdomainanimation/1967%20-%20Windy%20Day.ia.mp4" "testdata/raw/windy_day_1967.mp4"
# Eggs (1970) - animated short
fetch "https://archive.org/download/1960publicdomainanimation/1970%20-%20Eggs.ia.mp4" "testdata/raw/eggs_1970.mp4"
# Dinner Time (1928) - first sound-on-film cartoon (6 min)
fetch "https://upload.wikimedia.org/wikipedia/commons/1/19/Dinner_Time_%281928%29.webm" "testdata/raw/dinner_time_1928.webm"

for f in testdata/raw/bruder-1929.webm testdata/raw/cpidl-hallo.ogg testdata/raw/de-bier.ogg testdata/raw/dinner_time_1928.webm testdata/raw/the_singing_fool_1928.webm; do
  [[ -s "$f" ]] || { echo "missing $f"; exit 1; }
done
echo "assets ready in testdata/raw"