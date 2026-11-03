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

# Mirrors recorded 2026-11-03 after a download.blender.org outage aborted every
# job (this script marks sintel_trailer_2010.mp4 critical). Each alternate was
# checked to be the same cut and to decode through the pipeline; Wikimedia is a
# different host, so one outage no longer takes both down.
#
# A mirror is normalised to 720p H.264/AAC in the target's own container. The
# pipeline routes mp4/mov straight to ffmpeg and decodes by content, but keeping
# the container and resolution consistent means the benchmark timings stay
# comparable and nothing downstream sees a surprise codec.
#
# Usage: fetch_video_with_mirrors OUT PRIMARY MIRROR
fetch_video_with_mirrors(){
  local out="$1"; local primary="$2"; local mirror="$3"
  if [[ -s "$out" ]]; then echo "ok $out"; return 0; fi
  if curl -L --fail --connect-timeout 15 --max-time 600 -o "$out" "$primary" 2>/dev/null && [[ -s "$out" ]]; then
    echo "ok $out"
    return 0
  fi
  echo "primary unavailable, trying mirror for $out: $mirror"
  local tmp="${out}.mirror.ogv"
  rm -f "$tmp" "$out"
  if curl -L --fail --connect-timeout 15 --max-time 600 -o "$tmp" "$mirror" 2>/dev/null && [[ -s "$tmp" ]]; then
    if command -v ffmpeg >/dev/null 2>&1 && \
      ffmpeg -v error -y -i "$tmp" -vf scale=-2:720 -c:v libx264 -preset veryfast \
        -crf 23 -c:a aac -b:a 128k "$out" 2>/dev/null && [[ -s "$out" ]]; then
      echo "ok $out (mirror, re-encoded from $mirror)"
      rm -f "$tmp"
      return 0
    fi
    # No ffmpeg: keep the mirror bytes; ffmpeg still decodes them by content.
    mv "$tmp" "$out"
    echo "WARN: kept raw mirror bytes in $out (ffmpeg unavailable to normalise)"
    return 0
  fi
  rm -f "$tmp"
  echo "WARN: failed to fetch $out from $primary or $mirror"
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
fetch_video_with_mirrors \
  "testdata/raw/big_buck_bunny_trailer_2008.mov" \
  "https://download.blender.org/peach/trailer/trailer_480p.mov" \
  "https://upload.wikimedia.org/wikipedia/commons/b/b3/Big_Buck_Bunny_Trailer_400p.ogv"
# Sintel trailer (2010) - Blender Open Movie trailer
fetch_video_with_mirrors \
  "testdata/raw/sintel_trailer_2010.mp4" \
  "https://download.blender.org/durian/trailer/sintel_trailer-720p.mp4" \
  "https://upload.wikimedia.org/wikipedia/commons/0/06/Sintel_trailer-1080p.ogv"
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

  # Sintel (2010, CC-BY) - third film, never used for tuning. Its dialogue truth is
  # the English subtitle track embedded in the MKV (26 cues); 681 MB zip.
  fetch "https://download.blender.org/durian/movies/Sintel.2010.720p.mkv.zip" "testdata/raw/sintel_720p.zip"
  if [[ ! -s "testdata/raw/Sintel.2010.720p.mkv" && -s "testdata/raw/sintel_720p.zip" ]]; then
    python3 -c "import zipfile; zipfile.ZipFile('testdata/raw/sintel_720p.zip').extract('Sintel.2010.720p.mkv', 'testdata/raw')"
  fi
  if [[ ! -s "testdata/raw/sintel_2010.en.srt" && -s "testdata/raw/Sintel.2010.720p.mkv" ]]; then
    ffmpeg -v error -y -i testdata/raw/Sintel.2010.720p.mkv -map 0:s:m:language:eng -f srt testdata/raw/sintel_2010.en.srt || true
  fi
  cues="$(grep -c -- '-->' testdata/raw/sintel_2010.en.srt 2>/dev/null || true)"
  if [[ "${cues:-0}" != "26" ]]; then
    echo "WARN: sintel_2010.en.srt has ${cues:-0} cues, expected 26"
    FAIL_COUNT=$((FAIL_COUNT + 1))
  fi

  # Sprite Fright (2021, CC BY 4.0) - the only real film in the cue evaluation that
  # never informed any detector, cue, pad or threshold work. Dialogue truth is the
  # community timed text from Wikimedia Commons (80 cues); 159 MB webm.
  fetch "https://upload.wikimedia.org/wikipedia/commons/7/76/Sprite_Fright_-_Blender_Open_Movie-full_movie.webm" "testdata/raw/sprite_fright_2021.webm"
  fetch "https://commons.wikimedia.org/wiki/TimedText:Sprite_Fright_-_Blender_Open_Movie-full_movie.webm.en.srt?action=raw" "testdata/raw/sprite_fright_2021.en.srt"
  sprite_cues="$(grep -c -- '-->' testdata/raw/sprite_fright_2021.en.srt 2>/dev/null || true)"
  if [[ "${sprite_cues:-0}" != "80" ]]; then
    echo "WARN: sprite_fright_2021.en.srt has ${sprite_cues:-0} cues, expected 80"
    FAIL_COUNT=$((FAIL_COUNT + 1))
  fi
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
