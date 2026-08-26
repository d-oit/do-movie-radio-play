# GOAP State

**Current Goal**: Fix P0 speech-evidence slice panic for out-of-range segment timestamps (#222)
**Status**: In Progress

## Task Graph
- [x] Task G0: Issue #222 filed; code recon (`speech_evidence.rs` vs `segmenter/confidence.rs` clamp pattern)
- [x] Task G1: Clamp `start_idx`/`end_idx` in `average_frame_stats`, mirroring `segmenter/confidence.rs:53-58`
- [x] Task G2: Regression test — segment beyond decoded duration does not panic; deterministic output
- [x] Task G3: Local: `cargo test -p movie-radio-pipeline` 45/45 green; fmt clean; clippy `-p movie-radio-pipeline --all-features -D warnings` clean. Workspace-wide gate blocked locally by pre-existing `llama-cpp-sys` libclang header issue (CI covers it).
- [ ] Task G4: PR merged (squash), issue #222 closed

## Evidence Log
- `speech_evidence.rs:43-47`: `end_idx = min(len).max(start_idx+1)` exceeds len when `start_idx >= len` → `&frames[start..end_idx]` panics (range end out of bounds).
- Twin correct implementation: `segmenter/confidence.rs:53-58` clamps `start_idx` to `len.saturating_sub(1)` then `end_idx.clamp(start_idx + 1, len)`.
- Callers: `filter_implausible_speech_segments` wired into extraction pipeline via `pipeline/mod.rs`.
- Post-fix behavior: out-of-range segments evaluate against the final frame instead of crashing (matches `confidence_for_range` semantics).
- Analysis source: `plans/130-improvement-analysis-2026-08-25.md` item A1 (P0).

## History
- 2026-08-26: Incident recovery — direct-to-main pushes of the Jules #224 branch (stale base, pre-#223) reverted PR #223 content: speech-evidence clamp fix, plans/130 analysis doc, and tracking-doc refreshes. All restored here from 3efca37. Recommendation: enable "require pull request" branch protection to prevent stale-base direct pushes.
- 2026-08-26: #224 spectral-flux rewrite landed (analysis.rs 505 → 464 LOC) — resolves the LOC violation deferred in FOLLOWUPS.md.
- 2026-08-25: #222 filed from workspace-wide improvement analysis sweep.
- 2026-08-23: #206 complete (PR #209 → 076601b); audio.cpp spike complete (plans/audiocpp-tts-spike.md).
- 2026-08-23: PR sweep complete (#204 merged d64941d, #205 closed, #207 merged e4460af, #208 merged 2d07e78). Issue #206 filed from #205 salvage.
- 2026-08-23 (earlier sweep, archived): #204 merged after bot-push override; #207 fixed Clippy 1.98 breakage; root `src/` dead tree logged in FOLLOWUPS.md.
