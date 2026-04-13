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
for f in testdata/raw/bruder-1929.webm testdata/raw/cpidl-hallo.ogg testdata/raw/de-bier.ogg; do
  [[ -s "$f" ]] || { echo "missing $f"; exit 1; }
done
echo "assets ready in testdata/raw"
