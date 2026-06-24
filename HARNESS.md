# Harness Engineering

> Agent = Model + Harness. This document is the harness map for do-movie-radio-play.
> Based on: [Harness Engineering](https://martinfowler.com/articles/harness-engineering.html)

## Mental Model

The harness has two axes:

- **Feedforward (guides):** What to do *before* writing code — context, constraints, conventions
- **Feedback (sensors):** What fires *after* writing code — automated checks that catch violations

And two modes:

- **Computational:** Deterministic (clippy, tests, deny) — always trust the output
- **Inferential:** LLM-based (skill docs, agent context) — direction, not commands

## Feedforward Guides

### Inferential (read before coding)

| Guide | Path | Purpose |
|---|---|---|
| Agent contract | `AGENTS.md` | Root coding conventions, change workflow, quality gates |
| Skills index | `.agents/SKILLS.md` | Available executable task knowledge |
| Harness overview | `HARNESS.md` (this file) | How guides and sensors connect |
| Clippy intent | `.clippy.toml` | Linting philosophy and allowed exceptions |
| Dependency rules | `deny.toml` | Supply chain security and license policy |
| Architecture | `plans/` | Architecture Decision Records and roadmaps |

### Computational (structural constraints)

| Constraint | File | Enforced by |
|---|---|---|
| Unsafe code forbidden | `Cargo.toml` `[workspace.lints.rust]` | `rustc` |
| Max 500 LOC/file | `AGENTS.md` | Agent self-check |
| Conventional commits | `commitlint.config.cjs` | `commitlint` pre-commit hook |

## Feedback Sensors

### Computational (deterministic — always trust)

| Sensor | Trigger | Config | LLM Fix Hint |
|---|---|---|---|
| `cargo fmt --check` | pre-commit | `rustfmt.toml` | Run `cargo fmt --all` |
| `cargo clippy -D warnings` | pre-commit + CI | `.clippy.toml`, `Cargo.toml` lints | Fix all warnings; see `.clippy.toml` for allowed exceptions |
| `cargo deny check` | pre-commit + CI | `deny.toml` | Check crate layering diagram in `Cargo.toml` comments |
| `cargo test` | CI (`ci.yml`) | `Cargo.toml` | Fix failing tests before opening PR |
| `shellcheck` | pre-commit | `.shellcheckrc` | Fix shell script issues at severity=warning |
| `gitleaks` | CI (`ci.yml`) | `.gitleaks.toml` | Remove secrets; use env vars or `.env` |

### Inferential (LLM-based — use for direction)

| Sensor | Path | Purpose |
|---|---|---|
| Codacy quality review | `.codacy.yml`, CI | Code quality suggestions |
| Codecov coverage | `.codecov.yml`, CI | Coverage regression detection |

## Steering Loop

When any sensor fires **repeatedly** (>2 times in one sprint):

1. Identify the root cause category (maintainability / architecture / behaviour)
2. Update the corresponding **feedforward guide** to prevent recurrence
3. If no guide exists, create one in `.agents/skills/` using the `skill-creator` skill
4. Document the update in `CHANGELOG.md`

The steering loop closes the harness: sensors fire → humans and agents update guides → sensors fire less.

## Self-Correction Protocol for Agents

When a computational sensor fires:

1. Read the full error message — it includes a fix hint
2. Identify category: fmt / lint / test / arch / security
3. Apply the minimal fix (do not refactor unrelated code)
4. Re-run the specific sensor: `cargo clippy`, `cargo test`, etc.
5. Only commit when the sensor is green

## Agent-Optimised Error Output

The `scripts/harness-check.sh` wrapper runs each sensor and emits structured error output with `HARNESS VIOLATION` prefix and agent-parseable fix hints. Use it for richer feedback than raw sensor output:

```bash
bash scripts/harness-check.sh <fmt|clippy|deny|test|arch|all>
```

See `scripts/harness-check.sh` for the full sensor → hint mapping.
