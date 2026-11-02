# ADR-130: LLM-Backed Gap Narration

**Status**: Accepted
**Date**: 2026-11-02
**Issues**: #412

## Context

The GOAP narration stage is deterministic and tag-based. The narrator
backends in `crates/movie-radio-pipeline/src/narrator.rs` already talk to
OpenAI, Ollama and Anthropic, but the production narration path never used
them. Free-form model output cannot be synthesized directly: it can invent
visual facts, exceed the speech-free window, switch language or collapse into
filler that ADR-128 forbids.

## Decision

- `narrator.llm.enabled` (default `false`) opts into the LLM path. The
  deterministic generator stays the default and is still used for every
  window whenever the LLM path is off.
- Visual scene understanding takes precedence when `narrator.visual.provider`
  is not `none`; the LLM path covers acoustic narration.
- `NarratorConfig` is the single provider configuration. The GOAP stage builds
  its backend through `build_narrator_backend`, the same dispatch the CLI
  narrator uses; backend `temperature` and `max_tokens` now reach the
  provider request bodies.
- Every window sends a `NarrationRequest`: gap bounds, allowed duration,
  language, style, word/duration budget, acoustic evidence ids, adjacent
  context, optional visual evidence, forbidden filler and forbidden claims.
  The versioned prompt (`templates/narrator_gap_prompt.md`,
  `gap-narration-v1`) embeds the request as JSON inside an explicitly
  untrusted data block, so media-derived strings cannot become template
  directives.
- The model must answer with one JSON object:
  `{text, evidence_ids, uncertainty?, pronunciation?}`. `evidence_ids` must be
  non-empty and a subset of the ids the request offered.
- Before synthesis, `validate_response` rejects invalid JSON, empty text,
  word-budget or estimated-duration overflow, wrong language (German/English
  markers), banned filler, missing/unknown evidence ids, forbidden factual
  claims, and an immediate repetition of the previous line.
- Retries are bounded by `narrator.llm.max_attempts` (1-3). Invalid output is
  never synthesized: after the budget the gap stays unnarrated and the reason
  is written to `<output>.llm-narration.json`. Transport/provider failures use
  the deterministic acoustic description instead, labelled
  `AudioOnly { reason: "llm provider unavailable: ..." }`.
- ADR-128 rules still apply to both paths; regression fixtures under
  `testdata/narration/` cover generic filler, unsupported claims, injection
  attempts and German/English output.

## Consequences

- Enabling the LLM requires an API key (or a local Ollama endpoint) but no
  separate configuration tree; disabling it restores byte-identical
  deterministic narration.
- Every accepted line is auditable through evidence ids, prompt version,
  provider/model and the per-window review report.
- A model that keeps returning invalid JSON produces unnarrated gaps rather
  than invented text; operators see the reason in the report and logs.

## Evaluation Rubric

Human-reviewed fixtures are checked against six dimensions before a prompt or
validator change ships:

| Dimension | Check |
| --- | --- |
| Factual grounding | Every cited id exists and the text adds no unsupported claim |
| Relevance | Text describes the gap's own tags/neighbours, not the whole movie |
| Concision | Word count fits `max_words` and the estimated speech duration fits the window |
| Timing | `validate_response` estimates duration at the configured WPM |
| Language | Detected language matches `narrator.language` (German/English markers) |
| Non-repetition | The line differs from the immediately adjacent narration |

`testdata/narration/gap_response_de.json` and `gap_response_en.json` are the
accepted reference examples; `gap_response_prose_and_fence.txt` covers
prose-wrapped output and an instruction-injection attempt.
