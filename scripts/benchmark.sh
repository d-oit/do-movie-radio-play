#!/usr/bin/env bash
set -euo pipefail
input=${1:-testdata/generated/alternating.wav}
out=${2:-analysis/benchmarks/latest.json}
mkdir -p "$(dirname "$out")"
if [[ ! -s "$input" ]]; then
  mkdir -p "$(dirname "$input")"
  python - "$input" <<'PY'
import wave, struct, math, sys
path = sys.argv[1]
with wave.open(path, 'w') as w:
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(16000)
    for i in range(16000 * 4):
        t = i / 16000.0
        sample = 0.0 if int(t) % 2 == 0 else math.sin(2 * math.pi * 220 * t) * 0.25
        w.writeframes(struct.pack('<h', int(sample * 32767)))
PY
fi
cargo run --quiet -- bench "$input" --output "$out"
echo "benchmark written: $out"
