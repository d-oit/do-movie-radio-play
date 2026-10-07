#!/usr/bin/env bash
set -uo pipefail
mkdir -p testdata/raw
FAIL_COUNT=0
fetch(){
  local url="$1"; local out="$2"
  if [[ -s "$out" ]]; then echo "ok $out"; return 0; fi
  local attempt
  for attempt in 1 2 3; do
    if curl -L --fail --connect-timeout 15 --max-time 120 -o "$out" "$url" 2>/dev/null; then
      if [[ -s "$out" ]]; then echo "ok $out"; return 0; fi
    fi
    echo "retry $attempt for $out"
    sleep $((attempt * 5))
  done
  echo "WARN: failed to fetch $out from $url"
  FAIL_COUNT=$((FAIL_COUNT + 1))
  return 0
}

extract_first_mp4_from_zip(){
  local zip_path="$1"; local out="$2"
  if [[ -s "$out" ]]; then echo "ok $out"; return 0; fi
  python3 - <<'PY' "$zip_path" "$out"
import pathlib
import sys
import zipfile

zip_path = pathlib.Path(sys.argv[1])
out_path = pathlib.Path(sys.argv[2])

with zipfile.ZipFile(zip_path) as zf:
    mp4_names = [name for name in zf.namelist() if name.lower().endswith('.mp4')]
    if not mp4_names:
        raise SystemExit(f"no mp4 file found in archive: {zip_path}")
    with zf.open(mp4_names[0]) as src, out_path.open('wb') as dst:
        dst.write(src.read())
PY
  if [[ ! -s "$out" ]]; then
    echo "WARN: empty extracted movie: $out"
    FAIL_COUNT=$((FAIL_COUNT + 1))
  fi
}
# Layer 3: Post-2000 video fixtures only.
# Elephants Dream (2006) - Blender Open Movie
fetch "https://archive.org/download/ElephantsDream/ed_1024_512kb.mp4" "testdata/raw/elephants_dream_2006.mp4" || true
if [[ ! -s "testdata/raw/elephants_dream_2006.mp4" ]]; then
  fetch "https://download.blender.org/ED/elephantsdream-480-h264-st-aac.mov" "testdata/raw/elephants_dream_2006.mp4"
fi
# Multilingual subtitle fixtures for non-English validation coverage
fetch "https://commons.wikimedia.org/wiki/TimedText:Elephants_Dream_(2006).webm.es.srt?action=raw" "testdata/raw/elephants_dream_2006.es.srt"
fetch "https://commons.wikimedia.org/wiki/TimedText:Elephants_Dream_(2006).webm.de.srt?action=raw" "testdata/raw/elephants_dream_2006.de.srt"
# Big Buck Bunny trailer (2008) - Blender Open Movie trailer
fetch "https://download.blender.org/peach/trailer/trailer_480p.mov" "testdata/raw/big_buck_bunny_trailer_2008.mov"
# Sintel trailer (2010) - Blender Open Movie trailer
fetch "https://download.blender.org/durian/trailer/sintel_trailer-720p.mp4" "testdata/raw/sintel_trailer_2010.mp4"
# Elephants Dream teaser (2006) - Blender Open Movie teaser
fetch "https://download.blender.org/demo/movies/elephantsdream_teaser.mp4.zip" "testdata/raw/elephantsdream_teaser.mp4.zip"
extract_first_mp4_from_zip "testdata/raw/elephantsdream_teaser.mp4.zip" "testdata/raw/elephantsdream_teaser.mp4"
# Caminandes: Gran Dillama (2013) - Blender Foundation short
fetch "https://download.blender.org/demo/movies/caminandes_gran_dillama.mp4.zip" "testdata/raw/caminandes_gran_dillama.mp4.zip"
extract_first_mp4_from_zip "testdata/raw/caminandes_gran_dillama.mp4.zip" "testdata/raw/caminandes_gran_dillama.mp4"

# Tears of Steel (2012, CC-BY) - second validation film with timed dialogue truth (#364).
# Subtitles are small and always fetched; the 372 MB film only with FETCH_SECOND_FILM=1.
fetch "https://download.blender.org/demo/movies/ToS/subtitles/TOS-en.srt" "testdata/raw/tears_of_steel_2012.en.srt"
fetch "https://download.blender.org/demo/movies/ToS/subtitles/TOS-de.srt" "testdata/raw/tears_of_steel_2012.de.srt"
verify_sha256(){
  local file="$1"; local want="$2"
  [[ -s "$file" ]] || return 0
  local got; got="$(sha256sum "$file" | awk '{print $1}')"
  if [[ "$got" != "$want" ]]; then
    echo "WARN: checksum mismatch for $file (got $got)"
    FAIL_COUNT=$((FAIL_COUNT + 1))
  fi
}
verify_sha256 "testdata/raw/tears_of_steel_2012.en.srt" "ce9578f6fe098a5821045a38884b229160f9e7ba27890448a92f5be330c6e6fe"
verify_sha256 "testdata/raw/tears_of_steel_2012.de.srt" "666685daa9064a5a59144e47bddd2d570bbdda199df6e4e39f3273e479ab1f4b"
if [[ "${FETCH_SECOND_FILM:-0}" == "1" ]]; then
  fetch "https://download.blender.org/demo/movies/ToS/tears_of_steel_720p.mov" "testdata/raw/tears_of_steel_2012.mov"
fi

CRITICAL_ASSETS="testdata/raw/elephants_dream_2006.mp4 testdata/raw/sintel_trailer_2010.mp4"
ALL_ASSETS="testdata/raw/elephants_dream_2006.mp4 testdata/raw/elephants_dream_2006.es.srt testdata/raw/elephants_dream_2006.de.srt testdata/raw/big_buck_bunny_trailer_2008.mov testdata/raw/sintel_trailer_2010.mp4 testdata/raw/elephantsdream_teaser.mp4 testdata/raw/caminandes_gran_dillama.mp4"

missing_critical=0
for f in $CRITICAL_ASSETS; do
  if [[ ! -s "$f" ]]; then
    echo "CRITICAL: missing $f"
    missing_critical=1
  fi
done
if [[ "$missing_critical" -eq 1 ]]; then
  echo "aborting: critical assets missing"
  exit 1
fi

missing_optional=0
for f in $ALL_ASSETS; do
  if [[ ! -s "$f" ]]; then
    echo "WARN: optional asset missing $f"
    missing_optional=$((missing_optional + 1))
  fi
done

if [[ "$missing_optional" -gt 0 ]]; then
  echo "$missing_optional optional asset(s) missing; continuing with available assets"
fi
if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo "$FAIL_COUNT download(s) failed but all critical assets present"
fi
echo "assets ready in testdata/raw"
