# Contributing

Thanks for contributing to Movie Radio Play! Development is shared between
humans and automated agents; both follow the same rules (see `AGENTS.md` for
the canonical workflow and quality requirements).

## Workflow

1. Open an issue using the GitHub issue templates (`Coding Change`,
   `Performance Change`, or `Agent/Harness Change`).
2. Implement the change in minimal, atomic commits and run
   `bash scripts/quality_gate.sh` before pushing.
3. Open a pull request — every CI check (Quality Gate, CodeQL, Codacy,
   DeepSource, Repowise) must pass before it can merge.

## Issue & PR Triage Policy

The repository aims to keep **zero open issues and PRs**. To stay on top of
the queue:

- A weekly reminder workflow (`.github/workflows/triage-reminder.yml`) runs
  every **Monday 09:00 UTC** (also triggerable manually).
- It flags any open issue or PR with no activity in the last **7 days** by
  opening or updating a `triage`-labeled reminder issue.
- Triaging an item means: close no-impact issues with a brief rationale,
  address review comments, and merge PRs in dependency order — or explicitly
  defer them with a comment.

## Merge Rules

- `main` is protected: all required status checks must pass, and branches
  must be up to date before merging.
- Merges are squash merges; merged branches are deleted automatically.
- Force pushes and deletions on `main` are disabled.
