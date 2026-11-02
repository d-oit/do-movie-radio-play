#!/usr/bin/env python3
"""Deterministic tests for the cue evidence gate (no model, no media)."""

import sys

from cue_gate import keep_cue

FAILURES: list[str] = []


def check(label: str, got, want) -> None:
    if got != want:
        FAILURES.append(f"{label}: got {got!r}, want {want!r}")


def main() -> int:
    # A short shout with weak evidence survives: dropping it would hide speech.
    check("short weak cue kept", keep_cue(10.0, 11.5, 0.02, 0.1, 10.0), True)
    # A long low-evidence passage is the hallucination case the gate exists for.
    check("long weak cue dropped", keep_cue(30.0, 55.0, 0.02, 0.1, 10.0), False)
    # A long cue with real speech evidence is kept.
    check("long strong cue kept", keep_cue(30.0, 55.0, 0.9, 0.1, 10.0), True)
    # Boundary: exactly at the limit is treated as short.
    check("cue at limit kept", keep_cue(0.0, 10.0, 0.0, 0.1, 10.0), True)
    check("cue just over limit dropped", keep_cue(0.0, 10.01, 0.0, 0.1, 10.0), False)
    # Floor 0 disables the gate entirely.
    check("floor 0 keeps long weak cue", keep_cue(0.0, 60.0, 0.0, 0.0, 10.0), True)
    # Evidence exactly at the floor passes.
    check("evidence at floor kept", keep_cue(0.0, 60.0, 0.1, 0.1, 10.0), True)

    if FAILURES:
        print("cue gate tests failed:", file=sys.stderr)
        for failure in FAILURES:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print("cue gate tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
