# Initial Task Prompt (Archived)

```text
You are Codex running locally as an autonomous coding agent inside a repository.

You can inspect files, create and modify files, run shell commands, install project dependencies, fetch test assets when needed, run tests, run benchmarks, and iterate until the repository is complete and passing. Work as a disciplined software engineer, not as a chat assistant.

Your task is to build a production-grade Rust CLI called `movie-nonvoice-timeline`.

Goal
Build a CPU-only Rust CLI that extracts non-voice timeline regions from movie audio, outputs deterministic JSON, optionally tags those non-voice segments, optionally generates short listener-facing narration prompts, and includes a full agent-operable repository structure with AGENTS.md, .agents/skills/, plans/, analysis/, tests, logging, error handling, and benchmark support.

[Truncated in archive note: full prompt requested complete repo layout, CLI commands, deterministic testing, benchmark smoke, planning artifacts, and asset-fetch policy including Wikimedia references.]
```

## Operator note
The full original prompt was used during implementation and PR construction; this archive keeps the canonical objective and constraints summary in-repo for traceability.
