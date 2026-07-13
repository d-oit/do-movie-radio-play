# GOAP State

**Current Goal**: Implement issues #120, #122, #123 — render crate enhancement, ambient noise, and --preview CLI
**Status**: In-Progress

## Task Graph
- [x] Task 0: Analyze open issues and current codebase
- [ ] Task 1: Add workspace infra (playback feature, render workspace dep)
- [ ] Task 2: Implement noise.rs for ambient generation (Issue #122)
- [ ] Task 3: Implement PreviewOutput in movie-radio-io (Issue #123)
- [ ] Task 4: Enhance render crate (AGC, spatial, mixer - Issue #120)
- [ ] Task 5: Add Preview CLI subcommand and wire handler
- [ ] Task 6: Quality gate - cargo check, clippy, test, quality_gate.sh
- [ ] Task 7: PR with passing CI

## History
- 2026-07-13: Analyzed issues. movie-radio-render crate skeleton exists but needs enhancement. Needed: workspace dep for render, playback feature for rodio, noise.rs, preview output, CLI subcommand
