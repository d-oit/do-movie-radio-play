# ADR-129: Optional Visual Scene Understanding for Grounded Audio Description

**Status**: Accepted
**Date**: 2026-11-01
**Issues**: #409

## Context

`narrate::NarrationGenerator` describes only what the audio analysis detected
(acoustic tags, loudness trend, gap reason). That is deterministic and
audio-grounded (ADR-128), but it cannot describe visible events, actions,
scene transitions, positions or story information that a listener who cannot
see the film is missing. The product goal is audio description, not a
sound-effect commentary.

Adding a vision model is risky for a different reason: a general model happily
invents character names, dialogue, emotions and motives. Narration that
asserts such things is worse than saying less.

## Decision

Add an optional visual-understanding stage in
`crates/movie-radio-goap/src/narrate/visual/` with strict evidence provenance.
It is disabled by default (`narrator.visual.provider = "none"`), so existing
runs keep the acoustic path unchanged.

1. **Bounded frame sampling.** `FfmpegFrameExtractor` decodes only the candidate
   window (never the whole movie), prefers scene-change timestamps and fills
   the remaining budget evenly. `max_frames_per_window` (1-8) and
   `max_total_frames` cap decoding.
2. **Provider-agnostic vision interface.** `VisionProvider` is separate from the
   TTS and text-narrator backends. `OpenAiVision` targets any OpenAI-compatible
   `/chat/completions` endpoint; `OllamaVision` targets a local vision model
   (`/api/chat`) for offline use. Credentials come only from the environment
   variable named by `api_key_env`.
3. **Minimal context.** The request carries representative frames, timestamps,
   before/after scene context, this window's acoustic tags, known characters,
   narration language and the word budget.
4. **Structured, validated results.** `parse_analysis` requires JSON with
   `frame_ref`, `description`, `entities`, `actions`, `confidence` and
   `uncertainty`. Every observation must reference a supplied frame or a
   timestamp within 500 ms of one; timestamps are taken from the frame, never
   from model output.
5. **Observed facts only.** `grounding::ground` drops sentences and terms that
   attribute dialogue, emotion, motive or cause, name an unknown person, use
   "we see"/"the camera", or fall below `min_confidence`. Rejections and
   uncertainty notes are recorded per window. Only confident, grounded
   sentences become narration, fitted to the existing whole-clause word budget
   (ADR-128) and therefore to the speech-free window.
6. **Labelled fallback.** Every `NarrationScript` carries a `NarrationEvidence`
   value (`visual` with provider/model/frames/confidence, or `audio_only` with a
   reason). Windows that fail frame extraction, grounding, budget or provider
   calls keep the acoustic description and say why.
7. **Cache.** `FrameAnalysisCache` keys frames and analysis by movie identity,
   window, sampling policy, language and provider/model version, so repeated
   windows never re-decode or re-query.
8. **Review output.** `generate_narration` writes
   `<output>.visual-review.json` (and commits it as a `plan_narration`
   artifact) listing each window's source, frames with timestamps and
   confidence, rejected claims, narration text and fallback reason.

## Consequences

- Audio description can now mention visible events, but only those a sampled
  frame supports; the reviewer can trace every line back to frames.
- Vision costs are bounded and opt-in. A run without a provider behaves exactly
  as before, and a provider failure degrades to the acoustic description rather
  than failing the pipeline.
- `plan_narration` now depends on the source fingerprint as well as gaps and
  narration settings, and its stage version is bumped so pre-vision
  checkpoints are not reused.
- Grounding is deliberately conservative: some true statements are dropped
  (e.g. a correctly named character when no character roster is configured).
  That trade-off is documented and tested.
