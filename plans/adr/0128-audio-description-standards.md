# ADR-128: Audio-Description Standards for Narration Generation

**Status**: Accepted
**Date**: 2026-09-24
**Issues**: #344 (fix: PR #345)

## Context

The GOAP `generate_narration` action (`crates/movie-radio-goap/src/narrate/`)
bridges non-dialogue gaps in a radio play with synthesized narration. A
production run against a real German video showed the generator almost
always emitting content-free filler — `"Stille."` / `"Pause."` — instead of
describing what is actually happening. Root cause: `extract_context` never
read the gap segment's *own* detected tags (the exact signal the `tag`
pipeline stage computes for that span); it only inspected neighbouring
timeline entries, which in a persisted (inversion-only) timeline are
unrelated adjacent gaps, not real surrounding dialogue. The filler phrases
were also pushed into the candidate pool twice, so hash-selection favoured
them even when real tag signal was present.

There was no repository-level standard defining what "good" narration text
looks like, so the regression shipped past `cargo test`, `clippy`, and
`quality_gate.sh` — none of those gates check narration *content*, only
narration *code quality*.

## Decision

Adopt the German ARD/MDR/NDR/ORF/SRF/ZDF joint **Audiodeskription**
guidelines and the DCMP / W3C WAI **audio description** standards as the
normative reference for any code in this repository that generates
narration, gap-filling, or scene-description text (narrator, TTS-bridging,
SFX-caption text). Concretely, generated text MUST:

1. Describe what is actually audible or happening in the gap (grounded in
   the segment's own detected tags/context), never a content-free
   placeholder such as `"Stille."`, `"Pause."`, `"Schnitt."` on their own.
2. Use present tense, third person, objective/neutral wording — no
   interpretation of motives or unverifiable emotional claims.
3. Stand on its own whether or not a sound effect is actually rendered
   underneath it — the SFX library is optional and may be empty.
4. Stay deterministic for identical input (repo-wide rule; ADR-002/ADR-004
   already require this — no hashing/randomness in text selection).
5. Fit the available gap duration (word budget), preferring to omit a
   secondary clause over truncating mid-sentence.

Enforcement is layered (see `.agents/skills/audio-description-writer/SKILL.md`
for the full checklist):

- **Guardrail (runtime)**: `narrate::mod::reject_banned_filler` rejects any
  generated text that is *exactly* one of a small banned-filler list and
  substitutes the safe fallback clause, logging a warning. This makes the
  2026-09 regression structurally unrepeatable even if a future template
  reintroduces a bare filler phrase.
- **Guardrail (test)**: `narrate::tests::test_generate_text_is_grounded_not_filler`
  and `test_self_tags_take_precedence_over_neighbour_tags` assert grounding;
  any new narration source (LLM backend, template, provider) must add an
  equivalent test before merging.
- **Guardrail (review)**: `AGENTS.md` Rules and `HARNESS.md` Feedback
  Sensors reference this ADR so both human and agent reviewers check
  narration *content*, not just narration *code*.

## Consequences

- Narration quality becomes a first-class, testable property instead of an
  implicit assumption; new narration sources must justify themselves against
  this ADR before they ship.
- The runtime guardrail adds a small, allocation-free check to every
  generated script; negligible cost relative to synthesis.
- Future LLM-backed narration (ADR-126) must be prompted to follow the same
  rules; a prompt-template change alone cannot silently regress this
  guarantee because the runtime guardrail is provider-agnostic.
