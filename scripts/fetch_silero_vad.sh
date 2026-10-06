#!/usr/bin/env bash
# Fetch the Silero VAD v5 ONNX model (MIT) for `--features silero-vad`.
set -euo pipefail
OUT="${1:-models/silero_vad.onnx}"
URL="https://raw.githubusercontent.com/snakers4/silero-vad/v5.1.2/src/silero_vad/data/silero_vad.onnx"
mkdir -p "$(dirname "$OUT")"
[[ -s "$OUT" ]] || curl -L --fail --retry 3 -o "$OUT" "$URL"
echo "ok $OUT ($(wc -c <"$OUT") bytes)"
echo "also set ORT_DYLIB_PATH to an ONNX Runtime shared library (libonnxruntime.so)"
