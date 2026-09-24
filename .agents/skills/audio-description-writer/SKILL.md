---
name: audio-description-writer
description: Write or review radio-play narration and gap-filling text so it describes actual scene/audio content instead of content-free filler. Use when implementing or modifying narration generation (templates, LLM prompts, TTS-bridging text), or when reviewing generated narration output for quality.
---

# Audio-Description Writer

Ground any narration/gap-filling text this codebase generates in real,
detected content — never a content-free placeholder. Applies to
`crates/movie-radio-goap/src/narrate/`, `crates/movie-radio-pipeline/src/narrator.rs`
(LLM prompt templates), and any new narration source added later.

## When to use

- Implementing or modifying `NarrationGenerator::generate_text` or any
  successor (rule-based, template-based, or LLM-backed).
- Adding a new `NarratorAiBackend` (ADR-126) or editing
  `templates/narrator_prompt.md`.
- Reviewing a PR that touches narration output, or reviewing narration text
  produced by a production run before shipping a radio play.

## Background: this codebase regressed once already

A 2026-09 production run against a real German video showed the narrator
emitting `"Stille."` / `"Pause."` for almost every gap instead of describing
what was actually happening. Root cause: the context extractor read only
neighbouring timeline entries (which, in a persisted non-voice-only
timeline, are unrelated adjacent gaps) instead of the gap segment's own
detected tags. See [ADR-128](../../../plans/adr/0128-audio-description-standards.md)
and issue #344 / PR #345 for the full fix. This skill exists so the next
narration change doesn't reintroduce the same class of bug.

## Standard: what generated text MUST do

Derived from the German ARD/MDR/NDR/ORF/SRF/ZDF joint Audiodeskription
guidelines and the DCMP / W3C WAI audio-description standards:

1. **Describe, don't placeholder.** Name what is audible or happening —
   answer "was ist zu hören" (what is heard) — grounded in the segment's own
   detected tags (`crates/movie-radio-types` `Segment.tags`, produced by the
   `tag` pipeline stage) or gap `reason` string. Never emit a bare
   content-free word (`"Stille."`, `"Pause."`, `"Schnitt."`,
   `"Atmosphäre."` alone) as the entire narration.
2. **Present tense, third person, objective.** State observable facts, not
   interpreted motives or feelings the tags don't support.
3. **Stand alone.** The text must make sense whether or not a sound effect
   actually renders underneath it — the SFX library (`SfxManager`) is
   optional and may be empty or misconfigured at runtime.
4. **Stay deterministic.** No hashing/randomness in text selection for
   identical input (repo-wide rule, AGENTS.md "Deterministic output").
5. **Fit the budget.** Prefer omitting a secondary clause over truncating a
   sentence mid-word; never return empty text for a gap that passed the
   confidence/duration filters.

## Implementation pattern used in `narrate/mod.rs`

```rust
// 1. Ground: read the gap's OWN segment tags first, then neighbours.
let context = self.extract_context(timeline, gap); // context.self_tags first

// 2. Compose whole clauses from tag -> German descriptive sentence,
//    not word-salad or a single hashed template pick.
let chunks = Self::build_chunks(&context); // content_clause + reason_clause + duration_clause

// 3. Fit clauses to the word budget without truncating mid-sentence.
let text = self.fit_chunks_to_budget(&chunks, max_words);

// 4. Guardrail: never ship a banned filler phrase, even if a future
//    change reintroduces one.
let text = reject_banned_filler(text);
```

## Verification checklist (run before merging a narration change)

1. `cargo test -p movie-radio-goap narrate` — must include a test asserting
   output is `!= "Stille."` / `!= "Pause."` for a realistic tagged gap
   (pattern: `test_generate_text_is_grounded_not_filler`).
2. `cargo test -p movie-radio-goap narrate` — determinism test: same input
   twice must produce identical text (pattern:
   `test_generate_text_is_deterministic`).
3. Grep the new code path for the banned-filler constant list in
   `narrate/mod.rs` (`BANNED_FILLER_ONLY`) — any new template/backend output
   space must route through `reject_banned_filler` or an equivalent guard.
4. Run a real end-to-end check: `timeline radio-play <movie> --timeline <tagged.json> --output out.mp3`
   against a real or realistic fixture and read the logged
   `"Synthesizing narration"` `text=` fields — confirm each is a grounded
   sentence, not a single content-free word.
5. For LLM-backed narration (ADR-126 backends), the prompt template
   (`templates/narrator_prompt.md`) must instruct the same five rules above;
   a prompt change alone cannot bypass the runtime guardrail, but should
   still be reviewed against this checklist.

## Reference clause vocabulary (extend, don't replace, when adding tags)

| Detected tag | German descriptive clause |
| --- | --- |
| `impact_heavy` | "Ein kräftiger Aufprall ertönt." |
| `crowd_like` | "Eine Menschenmenge murmelt im Hintergrund." |
| `nature_like` | "Naturgeräusche sind zu hören." |
| `tonal` / `music_like` | "Melodische Klänge erfüllen den Raum." |
| `machinery_like` | "Ein gleichmäßiges Maschinengeräusch läuft mit." |
| `music_bed` | "Musik untermalt die Szene." |
| `ambience` | "Eine ruhige Klangkulisse liegt darüber." |
| `speech_like` | "Gedämpfte Stimmen sind zu hören." |

When adding a new acoustic tag anywhere in the pipeline (`pipeline/tags.rs`
or equivalent), add its clause here first, then wire it into
`content_clause` in `narrate/mod.rs` — keeps the vocabulary and the code in
sync for future contributors.
