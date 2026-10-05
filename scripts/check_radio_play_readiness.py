#!/usr/bin/env python3
import argparse
import json
import math
from pathlib import Path


def load_summary(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def metric_value(metrics: dict, key: str) -> float:
    aliases = {
        "non_voice_precision": ["non_voice_time_precision", "non_voice_precision"],
        "non_voice_recall": ["non_voice_time_recall", "non_voice_recall"],
    }
    for candidate in aliases.get(key, [key]):
        value = metrics.get(candidate)
        if value is not None:
            return float(value)
    value = metrics.get(key)
    if value is None:
        return 0.0
    return float(value)


METRICS = ("non_voice_precision", "non_voice_recall", "overlap_ratio")


def load_floors(path: Path) -> dict:
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8")).get("floors", {})


def raise_floors(floors: dict, checks: list) -> dict:
    """Ratchet: floors only move up; unseen entries start at their measured value."""
    out = {k: dict(v) for k, v in floors.items()}
    for check in checks:
        entry = out.setdefault(check["id"], {})
        for key in METRICS:
            entry[key] = math.floor(max(entry.get(key, 0.0), check[key]) * 1e4) / 1e4
    return out


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check holdout radio-play readiness from validation sweep summary"
    )
    parser.add_argument(
        "--summary",
        default="analysis/validation/full-sweep-summary.json",
        help="Validation sweep summary JSON",
    )
    parser.add_argument(
        "--holdout-tier",
        default="C",
        help="Tier to treat as holdout for readiness gate",
    )
    parser.add_argument(
        "--min-non-voice-precision",
        type=float,
        default=0.95,
        help="Minimum required non_voice_precision on holdout entries",
    )
    parser.add_argument(
        "--min-non-voice-recall",
        type=float,
        default=0.95,
        help="Minimum required non_voice_recall on holdout entries",
    )
    parser.add_argument(
        "--min-overlap",
        type=float,
        default=0.95,
        help="Minimum required overlap_ratio on holdout entries",
    )
    parser.add_argument(
        "--floors",
        help="Ratchet mode: per-entry metric floors JSON replacing the 0.95 targets "
        "(which stay the documented long-term ceiling)",
    )
    parser.add_argument(
        "--update-floors",
        action="store_true",
        help="With --floors, raise floors to current values (never lowers them)",
    )
    args = parser.parse_args()

    summary_path = Path(args.summary)
    if not summary_path.exists():
        raise FileNotFoundError(f"missing summary file: {summary_path}")

    summary = load_summary(summary_path)
    holdout_tier = args.holdout_tier.upper()
    results = summary.get("results", [])
    holdout_results = [
        r for r in results if str(r.get("tier", "")).upper() == holdout_tier
    ]
    if not holdout_results:
        raise ValueError(f"no holdout entries found for tier={holdout_tier}")

    failures = []
    checks = []
    floors = load_floors(Path(args.floors)) if args.floors else {}
    for result in holdout_results:
        metrics = result.get("metrics", {})
        precision = metric_value(metrics, "non_voice_precision")
        recall = metric_value(metrics, "non_voice_recall")
        overlap = metric_value(metrics, "overlap_ratio")
        entry_id = result.get("id", "unknown")

        checks.append(
            {
                "id": entry_id,
                "non_voice_precision": precision,
                "non_voice_recall": recall,
                "overlap_ratio": overlap,
            }
        )

        if args.floors:
            floor = floors.get(entry_id)
            if floor is None:
                failures.append(f"{entry_id}: no floor recorded (run with --update-floors)")
            else:
                for key in METRICS:
                    value = checks[-1][key]
                    if value < floor.get(key, 0.0):
                        failures.append(
                            f"{entry_id}: {key}={value:.4f} < floor {floor.get(key, 0.0):.4f}"
                        )
            continue
        if precision < args.min_non_voice_precision:
            failures.append(
                f"{entry_id}: non_voice_precision={precision:.4f} < {args.min_non_voice_precision:.4f}"
            )
        if recall < args.min_non_voice_recall:
            failures.append(
                f"{entry_id}: non_voice_recall={recall:.4f} < {args.min_non_voice_recall:.4f}"
            )
        if overlap < args.min_overlap:
            failures.append(
                f"{entry_id}: overlap_ratio={overlap:.4f} < {args.min_overlap:.4f}"
            )

    if args.floors and args.update_floors:
        # Only ratchet up when the gate currently holds, so a regression cannot lower the bar.
        if failures and floors:
            print("refusing to update floors while gate is red", file=__import__("sys").stderr)
        else:
            Path(args.floors).write_text(
                json.dumps({"floors": raise_floors(floors, checks)}, indent=2) + "\n",
                encoding="utf-8",
            )
            failures = []

    report = {
        "summary": str(summary_path),
        "holdout_tier": holdout_tier,
        "thresholds": {
            "min_non_voice_precision": args.min_non_voice_precision,
            "min_non_voice_recall": args.min_non_voice_recall,
            "min_overlap": args.min_overlap,
        },
        "mode": "ratchet" if args.floors else "target",
        "checks": checks,
        "passed": len(failures) == 0,
        "failures": failures,
    }

    print(json.dumps(report, indent=2))
    if failures:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
