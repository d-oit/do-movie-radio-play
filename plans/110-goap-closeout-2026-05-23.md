# GOAP Closeout — Resolve All Open Issues

**Date:** 2026-05-23  
**Status:** ✅ Complete — all 6 issues resolved, 5 PRs merged

## Dependency Graph

```
Group A (Blocker — sequential, security CVE)
  └── ✅ #54/#58 — Upgrade rustls-webpki >= 0.103.13 (DUPLICATE)

Group B (Parallel-safe — independent features)
  ├── ✅ #55 — Commit hybrid VAD engine (already in main)
  ├── ✅ #57 — Profile-driven tag calibration (Phase 6.1)
  ├── ✅ #59 — Close manifest coverage gap
  └── ✅ #60 — High-quality resampling via rubato (feature flag)

Group C (Depends on B)
  └── ✅ #56 — Add SpectralVad & HybridVad to benchmarks (depends on #55)
```

## Final Results

| # | Issue | Branch | PR | Status |
|---|-------|--------|----|--------|
| 1 | #54/#58 | `fix/issue-58-rustls-webpki` | [#61](https://github.com/d-oit/do-movie-radio-play/pull/61) | ✅ Removed vuln dep; #54 closed as dup |
| 2 | #55 | — | — | ✅ Already committed (d351f66) |
| 3 | #56 | `feat/issue-56-vad-benchmarks` | [#62](https://github.com/d-oit/do-movie-radio-play/pull/62) | ✅ Added SpectralVad + HybridVad to Criterion |
| 4 | #57 | `feat/issue-57-tag-calibration` | [#63](https://github.com/d-oit/do-movie-radio-play/pull/63) | ✅ TagRules + CLI wiring + tests |
| 5 | #59 | `fix/issue-59-manifest-coverage` | [#64](https://github.com/d-oit/do-movie-radio-play/pull/64) | ✅ Docs updated; coverage clean |
| 6 | #60 | `feat/issue-60-rubato-resample` | [#65](https://github.com/d-oit/do-movie-radio-play/pull/65) | ✅ Feature flag + rubato sinc resampler |

## Learnings

- libsql local usage doesn't need TLS — disabling default features removed 80+ transitive deps
- rubato 3.0 API uses `audioadapter` traits (`InterleavedOwned`, `Adapter`) — not the earlier `SincFixedInPlace`
- Async resampler output can vary by 1 sample vs exact ratio — tests must allow this
- Feature flags + cfg-gated tests can conflict — use `#[cfg(not(feature = "..."))]` consistently
- `testdata/raw/` is intentionally empty (media files downloaded separately) — docs updated to reflect this policy
