#!/usr/bin/env python3
"""
scripts/test-do-harness.py
Automated sensor selection and path mapping test for do-harness.toml.
"""

import sys
import fnmatch
from pathlib import Path

try:
    import tomllib
except ImportError:
    import tomli as tomllib

REPO_ROOT = Path(__file__).resolve().parent.parent
HARNESS_TOML = REPO_ROOT / "do-harness.toml"


def match_path_glob(path_str: str, glob_pattern: str) -> bool:
    """Matches a file path string against a glob pattern supporting **."""
    if glob_pattern.startswith("./"):
        glob_pattern = glob_pattern[2:]
    if path_str.startswith("./"):
        path_str = path_str[2:]

    # Convert ** to fnmatch pattern
    pattern = glob_pattern.replace("**/", "MATCHALL_DIR_").replace("**", "MATCHALL_ANY")
    pattern = fnmatch.translate(pattern)
    pattern = pattern.replace("MATCHALL_DIR_", "(?:.*/)?").replace("MATCHALL_ANY", ".*")
    import re
    return re.match(pattern, path_str) is not None


def resolve_affected_crates(file_path: str, workspace_deps: dict) -> set[str]:
    """Resolves all crates directly or indirectly affected by a file change."""
    affected = set()
    for base_crate, dependents in workspace_deps.items():
        if file_path.startswith(base_crate + "/") or file_path == base_crate:
            affected.add(base_crate)
            for dep in dependents:
                affected.add(dep)
    return affected


def select_signal_sets(file_paths: list[str], harness_data: dict, opt_in_requested: set[str] = None) -> list[str]:
    if opt_in_requested is None:
        opt_in_requested = set()

    selected = []
    signal_sets = harness_data.get("signal_sets", {})

    for set_name, set_config in signal_sets.items():
        is_opt_in = set_config.get("opt_in", False)
        if is_opt_in and set_name not in opt_in_requested:
            continue

        patterns = set_config.get("paths", [])
        matched = False
        for path in file_paths:
            for pattern in patterns:
                if match_path_glob(path, pattern):
                    matched = True
                    break
            if matched:
                break

        if matched:
            selected.append(set_name)

    return selected


def main():
    if not HARNESS_TOML.exists():
        print(f"❌ Error: {HARNESS_TOML} does not exist.")
        sys.exit(1)

    with open(HARNESS_TOML, "rb") as f:
        harness_data = tomllib.load(f)

    print("▶ Testing do-harness.toml sensor selection & workspace mappings...")

    workspace_deps = harness_data.get("workspace_dependencies", {})

    # Test Case 1: Shared types crate change
    types_file = "crates/movie-radio-types/src/lib.rs"
    types_signals = select_signal_sets([types_file], harness_data)
    assert "feedback" in types_signals, f"Expected 'feedback' in {types_signals}"
    assert "verification" in types_signals, f"Expected 'verification' in {types_signals}"
    assert "benchmarks" not in types_signals, "Opt-in benchmarks should not be selected by default"
    affected_by_types = resolve_affected_crates(types_file, workspace_deps)
    assert "crates/movie-radio-pipeline" in affected_by_types, "Pipeline should be affected by types change"
    assert "crates/movie-radio-timeline" in affected_by_types, "Timeline should be affected by types change"
    print("  ✓ Case 1: Shared types change correctly triggers broad workspace verification")

    # Test Case 2: Pipeline crate change
    pipeline_file = "crates/movie-radio-pipeline/src/pipeline/mod.rs"
    pipeline_signals = select_signal_sets([pipeline_file], harness_data)
    assert "feedback" in pipeline_signals, f"Expected 'feedback' in {pipeline_signals}"
    assert "verification" in pipeline_signals, f"Expected 'verification' in {pipeline_signals}"
    affected_by_pipeline = resolve_affected_crates(pipeline_file, workspace_deps)
    assert "crates/movie-radio-timeline" in affected_by_pipeline, "Timeline affected by pipeline change"
    print("  ✓ Case 2: Pipeline change correctly triggers pipeline dependents")

    # Test Case 3: Audio render crate change + opt-in benchmark
    render_file = "crates/movie-radio-render/src/mixer.rs"
    render_signals_default = select_signal_sets([render_file], harness_data)
    assert "benchmarks" not in render_signals_default, "Opt-in benchmarks should not run automatically"
    render_signals_opt_in = select_signal_sets([render_file], harness_data, opt_in_requested={"benchmarks"})
    assert "benchmarks" in render_signals_opt_in, "Opt-in benchmarks should run when requested"
    print("  ✓ Case 3: Render change respects opt-in benchmark signal set")

    # Test Case 4: Validation crate change
    validation_file = "crates/movie-radio-validation/src/lib.rs"
    val_signals = select_signal_sets([validation_file], harness_data)
    assert "feedback" in val_signals and "verification" in val_signals
    print("  ✓ Case 4: Validation crate change selects feedback & verification")

    # Test Case 5: Workflow file change
    workflow_file = ".github/workflows/ci.yml"
    wf_signals = select_signal_sets([workflow_file], harness_data)
    assert "verification" in wf_signals, f"Expected 'verification' in {wf_signals}"
    assert "feedback" not in wf_signals, "Workflows should not trigger fast rust fmt/check"
    print("  ✓ Case 5: Workflow file change triggers verification signal set")

    # Test Case 6: VERSION file change
    version_file = "VERSION"
    v_signals = select_signal_sets([version_file], harness_data)
    assert "release" in v_signals, f"Expected 'release' in {v_signals}"
    print("  ✓ Case 6: VERSION change triggers release signal set")

    print("\n✅ All do-harness sensor selection tests PASSED!")


if __name__ == "__main__":
    main()
