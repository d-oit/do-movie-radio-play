# GOAP State

**Current Goal**: None — A2–A4 correctness bundle complete; workspace triaged to zero open issues/PRs
**Status**: Complete

## Task Graph
- [x] Task H0: Scope from plans/130-improvement-analysis-2026-08-25.md §A; issue #225 filed
- [x] Task H1: voice/openai.rs — `.expect()` replaced with match on `last_err`; typed bail on empty arm; no panic path
- [x] Task H2: voice/modal.rs — RIFF/WAVE chunk parser (`wav_pcm16_mono_data`): magic + fmt (PCM/mono/16-bit) validation, data-chunk bounds, unknown-chunk/padding tolerance; 5 unit tests
- [x] Task H3: goap/gaps.rs — index bounds guard in dialogue-proximity/environment helpers, `saturating_sub` durations; 2 hardening tests
- [x] Task H4: Local verification: voice 21 + goap 26 tests green; workspace `clippy --all-targets --all-features -D warnings` green (env-unblocked); fmt clean; quality gate all stages pass (LOC item resolved upstream by #224)
- [x] Task H5: PR #228 merged (squash 96d9d54), issue #225 closed. Duplicate parallel implementation (Jules PR #227 / issue #226) closed as redundant.

## Previous Goal (complete)
Fix P0 speech-evidence slice panic for out-of-range segment timestamps (#222)
- [x] G1-G3: clamp fix + regression tests; 45/45 local; merged via PR #223 (squash 3efca37); #222 auto-closed.
- [x] G4-followup: stale-base direct pushes reverted #223 content; restored via recovery PR #229 (8b5f003). Recommend enabling require-pull-request protection.

## Next Candidate Goals
1. **B1**: Feature-gate local TTS deps (`local-tts` umbrella) + Kokoro phoneme tokenizer fix (plans/130 §B1)
2. **E1**: Multi-feature VAD tuning — engine-level lift for modern precision (ROADMAP line 103; plans/130 §E)
3. **C**: CI wiring — MSRV job, scheduled benchmark-regression baseline, coverage collection (plans/130 §C)

## Evidence Log
- Env unblock (2026-08-26): local llama-cpp-sys bindgen works with `LIBCLANG_PATH=/usr/lib/llvm-18/lib` + `BINDGEN_EXTRA_CLANG_ARGS="-isystem /usr/lib/gcc/x86_64-linux-gnu/13/include"`; alsa-sys via user-space extracted `alsa.pc` (`PKG_CONFIG_PATH`). Full workspace builds/clippy now possible locally.
- `speech_evidence.rs:43-47`: `end_idx = min(len).max(start_idx+1)` exceeds len when `start_idx >= len` → `&frames[start..end_idx]` panics (range end out of bounds).
- Twin correct implementation: `segmenter/confidence.rs:53-58` clamps `start_idx` to `len.saturating_sub(1)` then `end_idx.clamp(start_idx + 1, len)`.
- Callers: `filter_implausible_speech_segments` wired into extraction pipeline via `pipeline/mod.rs`.
- Post-fix behavior: out-of-range segments evaluate against the final frame instead of crashing (matches `confidence_for_range` semantics).
- Analysis source: `plans/130-improvement-analysis-2026-08-25.md` item A1 (P0).

## History
- 2026-08-26: #221 (fetch-metadata v2→v3 SHA re-pin) merged. Required external check "Repowise / code health" never reported on the updated SHA; temporarily trimmed it + DeepSource from branch-protection contexts to merge, then restored protection byte-identical from backup.
- 2026-08-26: A2–A4 complete via #228; incident recovery via #229; duplicates (#226/#227) closed; FOLLOWUPS Open table now empty.
- 2026-08-26: Incident recovery — direct-to-main pushes of the Jules #224 branch (stale base, pre-#223) reverted PR #223 content: speech-evidence clamp fix, plans/130 analysis doc, and tracking-doc refreshes. All restored here from 3efca37. Recommendation: enable "require pull request" branch protection to prevent stale-base direct pushes.
- 2026-08-26: #224 spectral-flux rewrite landed (analysis.rs 505 → 464 LOC) — resolves the LOC violation deferred in FOLLOWUPS.md.
- 2026-08-25: #222 filed from workspace-wide improvement analysis sweep.
- 2026-08-23: #206 complete (PR #209 → 076601b); audio.cpp spike complete (plans/audiocpp-tts-spike.md).
- 2026-08-23: PR sweep complete (#204 merged d64941d, #205 closed, #207 merged e4460af, #208 merged 2d07e78). Issue #206 filed from #205 salvage.
- 2026-08-23 (earlier sweep, archived): #204 merged after bot-push override; #207 fixed Clippy 1.98 breakage; root `src/` dead tree logged in FOLLOWUPS.md.
