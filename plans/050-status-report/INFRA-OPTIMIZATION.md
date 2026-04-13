# Infrastructure and Workflow Optimization

Findings from reviewing scripts/, .agents/skills/, .github/workflows/, AGENTS.md,
and git hooks.

## scripts/ Issues

### pre-commit-hook.sh: Missing safety flags

`scripts/pre-commit-hook.sh:1-2` uses `#!/bin/bash` with `set -e` only.
All other scripts use `set -euo pipefail`. Missing `-u` (unset variable errors)
and `-o pipefail` (pipeline failure propagation) reduces safety.

**Fix:** Change line 2 to `set -euo pipefail`.

### pre-commit-hook.sh: Duplicates quality_gate.sh

`scripts/pre-commit-hook.sh` runs the exact same three commands as
`scripts/quality_gate.sh` (fmt, clippy, test) but with added echo statements.
This is maintenance duplication.

**Fix:** Replace body with `exec bash scripts/quality_gate.sh`.

### setup-skills.sh: Does nothing useful

`scripts/setup-skills.sh` only lists skill directory names. It does not
validate, install, or configure anything. The name implies setup functionality
that does not exist.

**Fix:** Either remove or give it actual setup responsibility (e.g., checking
SKILL.md structure, validating frontmatter).

### fetch_test_assets.sh: Validation loop references missing file

`scripts/fetch_test_assets.sh:19` validates `testdata/raw/nosferatu-1922.webm`
but this file is never fetched by the script. The `fetch` calls download
`bruder-1929.webm`, `cpidl-hallo.ogg`, `de-bier.ogg`, and
`the_city_slicker_1918.webm` -- but not nosferatu. This causes the script
to always fail at the validation step.

**Fix:** Either add a `fetch` call for nosferatu or remove it from the
validation loop.

### benchmark.sh: No Criterion integration

`scripts/benchmark.sh` runs `cargo run -- bench` (the CLI's built-in bench
command). There is no integration with Criterion or any statistical benchmark
framework. The `benches/` directory contains only a println stub.

**Fix:** See Phase 06 - add Criterion benchmarks alongside the CLI bench command.

## CI Workflow Issues

### ci.yml: Redundant quality gate step

`.github/workflows/ci.yml:13` runs `bash scripts/quality_gate.sh` after already
running the same three commands individually (lines 10-12: build, fmt, clippy, test).
This doubles the execution time for no benefit.

**Fix:** Either run only `quality_gate.sh` or only the individual steps, not both.
The individual steps give better CI error reporting; prefer those and drop line 13.

### ci.yml: Missing cargo cache

No `actions/cache` step for Cargo artifacts. Every CI run recompiles from scratch.

**Fix:** Add Rust caching:
```yaml
- uses: Swatinem/rust-cache@v2
```

### ci.yml: No benchmark smoke in CI

The quality gate in CI does not run `scripts/benchmark.sh`. AGENTS.md lists it
as part of the quality gate, but CI does not execute it.

**Fix:** Add benchmark smoke step after tests (requires fixture generation).

### ci.yml: No branch/path filtering

CI triggers on all pushes and PRs with no path filtering. Documentation-only
changes trigger a full Rust build.

**Fix:** Add path filters to skip CI for docs-only changes:
```yaml
on:
  push:
    paths-ignore: ['*.md', 'plans/**', 'analysis/**']
```

## .agents/skills/ Issues

### Irrelevant skills bloat (25 skills, ~7 project-relevant)

The project has 25 skills installed. Only about 7 are domain-relevant:
`audio-vad-cpu`, `nonvoice-segmentation`, `self-learning-calibration`,
`triz-audio-timeline`, `learn`, `analysis-swarm`, `atomic-commit`.

The remaining 18 are generic toolbox skills (database-devops, anti-ai-slop,
dogfood, cicd-pipeline, etc.) that add context noise without project benefit.

**Assessment:** Not a functional issue but increases cognitive load and context
window usage when skills are loaded. Consider whether generic skills should live
in a global skills directory rather than per-project.

### docs-hook/scripts/docs-sync.sh: Broken path calculation

`docs-sync.sh:9` computes `REPO_ROOT` by navigating four directories up from its
own location (`../../../..`). But the script lives at
`.agents/skills/docs-hook/scripts/docs-sync.sh` which is 4 levels deep. The
calculation goes to the parent of the repo root, not the repo root itself.

Correct path should be `../../..` (3 levels: scripts -> docs-hook -> skills -> .agents -> repo root is 4, but `dirname` returns the scripts dir, so `../../../..` goes one too far).

Actually: `BASH_SOURCE[0]` = `.agents/skills/docs-hook/scripts/docs-sync.sh`.
`dirname` = `.agents/skills/docs-hook/scripts`. Going `../../../..` from there
= parent of repo root. This is wrong.

**Fix:** Change to `"$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"`
should be `"$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../" && pwd)"` or better,
use `git rev-parse --show-toplevel`.

### docs-hook/scripts/docs-sync.sh: Broken if/elif nesting

`docs-sync.sh:89` has an `if` block nested inside an unclosed `elif` at line 78.
The `if [[ "$file" == agents-docs/* ]]` at line 89 is inside the `elif [[ "$file" == */SKILL.md ]]` block (line 84), but should be a separate top-level condition.
This means the agents-docs skip logic only triggers for SKILL.md files.

**Fix:** Close the `elif` block before the agents-docs check, or restructure
as a case statement.

### docs-hook/scripts/docs-sync.sh: Targets nonexistent directory

The script syncs to `$REPO_ROOT/agents-docs/` which does not exist in this
repository. The `AGENTS_DIR` default points to a directory that was never created.

**Fix:** Either create `agents-docs/` or update the default to a directory that
exists (e.g., `analysis/` or remove the sync target).

### docs-hook: Not actually hooked

The `.git/hooks/post-commit` only contains the Qoder tracker. `docs-sync.sh` is
not registered as a git hook anywhere. The SKILL.md claims it should be added to
post-commit, but it never was.

**Fix:** Either integrate into post-commit hook or remove the docs-hook skill
if it is not being used.

### atomic-commit/SKILL.md: Stale reference

`atomic-commit/SKILL.md:139` references `.opencode/commands/commit.md` and
`.github/PULL_REQUEST_TEMPLATE.md` -- neither file exists in this repository.

**Fix:** Remove or update the "See Also" references.

### learn skill: References nonexistent files

`learn/SKILL.md:55-56` references `agents-docs/LESSONS.md` and
`agents-docs/ENVIRONMENT_VARIABLES.md` (line 22). Neither exists.

**Fix:** Update references to actual project paths or remove.

## AGENTS.md Issues

### Missing lint/typecheck commands

AGENTS.md lists `cargo build` under Setup and `bash scripts/quality_gate.sh`
under Quality gate, but does not explicitly list lint and typecheck commands.
Agents need to know to run `cargo clippy` and `cargo fmt --check` independently.

**Fix:** Add explicit section:
```
## Lint and typecheck
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
```

### Missing test command

No explicit `cargo test` entry outside of quality_gate reference.

**Fix:** Add under Quality gate or separate Testing section.

### Missing repository map entries

`reports/`, `testdata/`, `benches/`, `.github/`, `scripts/` are not in the
repository map. Agents cannot discover these directories.

**Fix:** Add missing entries:
```
- `scripts/` quality gate, benchmark, test asset scripts
- `tests/` integration tests
- `benches/` benchmark stubs (not yet functional)
- `testdata/` generated fixtures and raw test assets
- `reports/` validation reports
- `.github/` CI workflows
```

### Missing rules

Several implicit project rules are not documented:
- No `unwrap()`/`expect()` in production code
- 16-bit PCM WAV only for direct reader
- Deterministic output requirement for all pipeline stages
- ffmpeg required on PATH for non-WAV formats

**Fix:** Add to Rules section.

## .gitignore Issues

### Redundant patterns

Lines 5 (`testdata/generated/*.wav`) and 7 (`testdata/generated/*`) overlap --
line 7 already covers everything line 5 matches.

**Fix:** Remove line 5.
