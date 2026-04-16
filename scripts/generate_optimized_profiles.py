#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def find_candidate(report: dict, candidate_name: str) -> dict:
    for row in report.get("ranked_candidates", []):
        candidate = row.get("candidate", {})
        if candidate.get("name") == candidate_name:
            return candidate
    raise ValueError(f"candidate not found in sweep report: {candidate_name}")


def apply_candidate_to_profile(base: dict, candidate: dict, profile_name: str) -> dict:
    out = dict(base)
    out["name"] = profile_name
    out["description"] = f"Auto-generated from FP sweep candidate '{candidate['name']}'"
    out["vad_engine"] = "spectral"

    extract = candidate.get("extract", {})
    verify = candidate.get("verify", {})

    threshold = extract.get("threshold")
    min_silence_ms = extract.get("min_silence_ms")
    if threshold is not None:
        out["energy_threshold"] = float(threshold)
    if min_silence_ms is not None:
        out["min_non_voice_ms"] = int(min_silence_ms)

    entropy_min = verify.get("entropy_min")
    flatness_max = verify.get("flatness_max")
    centroid_min = verify.get("centroid_min")
    if entropy_min is not None:
        out["spectral_entropy_min"] = float(entropy_min)
    if flatness_max is not None:
        out["spectral_flatness_max"] = float(flatness_max)
    if centroid_min is not None:
        out["spectral_centroid_min"] = float(centroid_min)
    out.setdefault("spectral_centroid_max", 6000.0)

    return out


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate modern/legacy optimized profiles from FP sweep report"
    )
    parser.add_argument(
        "--sweep-report",
        default="analysis/optimization/fp-sweep-ranked.json",
        help="Ranked sweep report JSON",
    )
    parser.add_argument(
        "--base-profile",
        default="config/profiles/radio-play.json",
        help="Base profile JSON to clone and patch",
    )
    parser.add_argument(
        "--modern-output",
        default="config/profiles/modern-optimized.json",
        help="Output profile path for modern content",
    )
    parser.add_argument(
        "--legacy-output",
        default="config/profiles/legacy-optimized.json",
        help="Output profile path for legacy content",
    )
    args = parser.parse_args()

    sweep_path = Path(args.sweep_report)
    base_profile_path = Path(args.base_profile)
    modern_output = Path(args.modern_output)
    legacy_output = Path(args.legacy_output)

    if not sweep_path.exists():
        raise FileNotFoundError(f"missing sweep report: {sweep_path}")
    if not base_profile_path.exists():
        raise FileNotFoundError(f"missing base profile: {base_profile_path}")

    sweep_report = load_json(sweep_path)
    base_profile = load_json(base_profile_path)

    policy = sweep_report.get("recommended_policy", {})
    modern_candidate_name = policy.get("modern_candidate")
    legacy_candidate_name = policy.get("legacy_candidate")
    if not modern_candidate_name or not legacy_candidate_name:
        raise ValueError("sweep report missing recommended modern/legacy policy")

    modern_candidate = find_candidate(sweep_report, modern_candidate_name)
    legacy_candidate = find_candidate(sweep_report, legacy_candidate_name)

    modern_profile = apply_candidate_to_profile(
        base_profile, modern_candidate, "modern-optimized"
    )
    legacy_profile = apply_candidate_to_profile(
        base_profile, legacy_candidate, "legacy-optimized"
    )

    modern_output.parent.mkdir(parents=True, exist_ok=True)
    legacy_output.parent.mkdir(parents=True, exist_ok=True)
    modern_output.write_text(
        json.dumps(modern_profile, indent=2) + "\n", encoding="utf-8"
    )
    legacy_output.write_text(
        json.dumps(legacy_profile, indent=2) + "\n", encoding="utf-8"
    )

    print(f"wrote modern profile: {modern_output}")
    print(f"wrote legacy profile: {legacy_output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
