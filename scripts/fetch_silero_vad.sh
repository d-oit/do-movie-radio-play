#!/usr/bin/env bash
# Fetch the Silero VAD ONNX model (MIT) for `--features silero-vad`.
# Pinned to the release every measurement in plans/GOAP_STATE.md used.
set -euo pipefail
OUT="${1:-models/silero_vad.onnx}"
URL="https://raw.githubusercontent.com/snakers4/silero-vad/v6.2.3/src/silero_vad/data/silero_vad.onnx"
SHA256="1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3"
mkdir -p "$(dirname "$OUT")"
if [[ ! -s "$OUT" ]]; then
  curl -L --fail --retry 3 -o "$OUT" "$URL"
fi
GOT="$(sha256sum "$OUT" | awk '{print $1}')"
if [[ "$GOT" != "$SHA256" ]]; then
  echo "checksum mismatch for $OUT (got $GOT)" >&2
  rm -f "$OUT"
  exit 1
fi
echo "ok $OUT ($(wc -c <"$OUT") bytes)"
echo "also set ORT_DYLIB_PATH to an ONNX Runtime shared library (libonnxruntime.so)"
