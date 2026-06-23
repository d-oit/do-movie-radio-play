---
name: goap-agent
description: Multi-step GOAP planning for complex audio pipeline development. Orchestrates analysis, decomposition, strategy selection, and execution with state persistence in plans/.
---

# GOAP Agent

Orchestrate complex, multi-step development tasks using Goal-Oriented Action Planning (GOAP). This skill is specifically tuned for audio processing pipelines and radio play production.

## When to use

- Implementing new milestones from the `ROADMAP.md`.
- Large-scale refactors across multiple modules (e.g., `src/pipeline` to `src/goap`).
- Tasks requiring sequential dependency management and quality gates between phases.
- When you need to maintain state across multiple agent sessions.

## Planning Workflow

1.  **Analyze**: Review existing `ROADMAP.md`, `ADR-INDEX.md`, and relevant `src/` code to understand the goal.
2.  **Decompose**: Break the goal into discrete, manageable tasks. **Requirement**: If the change introduces new architecture, an ADR must be created and approved before proceeding.
3.  **Strategize**: Select an execution strategy (Sequential, Parallel, or Hybrid) and map dependencies.
4.  **Coordinate**: Assign tasks to specialized agent roles (e.g., `feature-implementer`, `test-runner`).
5.  **Execute**: Perform the work according to the strategy, updating state after each step.
6.  **Synthesize**: Merge results, verify against the original goal, and close the plan.

## Persistent State

All GOAP planning state must be persisted in `plans/GOAP_STATE.md`. This allows for resumption across sessions and provides a clear audit trail.

```markdown
# GOAP State

**Current Goal**: <Goal Description>
**Status**: [Planned | In-Progress | Blocked | Complete]

## Task Graph
- [ ] Task 1 (Depends on: None)
- [ ] Task 2 (Depends on: Task 1)

## History
- <Timestamp>: <Action Taken> -> <Result>
```

## Quality Gates

Each step in the plan must pass:
1. `cargo check` and `cargo clippy`
2. `cargo test` for relevant modules
3. `bash scripts/quality_gate.sh` (if applicable)
4. Manual verification of output artifacts (e.g., JSON schema, audio headers)

## Examples

### Scenario A: Implementing Milestone D (Visual Gap Identification)

**Goal**: Identify moments in audio that require narration.

1.  **Analyze**: Read `ADR-120` and `plans/120-goap-radio-play-pipeline/ROADMAP.md`.
2.  **Decompose**:
    - Define gap types in `crates/movie-radio-types/`.
    - Implement gap detection heuristics in `crates/movie-radio-goap/src/gaps.rs`.
    - Add `--analyze-only` flag to CLI (`crates/movie-radio-timeline/src/cli.rs`).
3.  **Strategize**: Sequential. Heuristics depend on the data structures.
4.  **Execute**: Implement types, then gaps module.
5.  **Verify**: Run `timeline radio-play --analyze-only` on `testdata/generated/alternating.wav`.

### Scenario B: Adding a New TTS Provider (Milestone F)

**Goal**: Add Qwen3-TTS as a German-primary synthesis provider.

1.  **Analyze**: Read `ADR-121`.
2.  **Decompose**:
    - Implement `VoiceSynthesizer` trait for Qwen3 in `crates/movie-radio-voice/src/voice/qwen3.rs`.
    - Add provider-specific configuration to `crates/movie-radio-voice/src/config.rs`.
    - Create unit tests with mock API/model output.
3.  **Strategize**: Parallel-safe (Config and Trait implementation can happen in any order).
4.  **Execute**: Update config, then implement provider.
5.  **Synthesize**: Verify end-to-end synthesis in a test case.

## Rules
- **No Progress Without Approval**: New architecture requires an ADR in `plans/` before code changes.
- **State First**: Update `plans/GOAP_STATE.md` before starting any new task.
- **Atomic Commits**: Use `bash scripts/ai-commit.sh` for each completed task in the plan.
