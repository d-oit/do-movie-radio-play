Energy-threshold VAD is sufficient for non-voice windowing baseline; future work can add optional ONNX backend behind feature flag.
Post-2000 fixtures now act as primary inputs for smoke/validation/benchmark flows because they reduce variance from early-film audio artifacts; legacy fixtures remain fallback-only for compatibility.
Multilingual subtitle fixtures improve evaluation realism without sacrificing deterministic offline tests, and CI reliability improves when benchmark input is pinned and dependency updates are automated via guarded Dependabot auto-merge.
