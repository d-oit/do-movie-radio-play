# ADR-131: LLM Provider Policy — OpenRouter, Ollama, merge.dev Only

**Status**: Accepted
**Date**: 2026-11-04
**Supersedes**: the direct-provider defaults in ADR-126

## Context

Narration (`crates/movie-radio-pipeline/src/narrator.rs`) and visual scene
understanding (`crates/movie-radio-goap/src/narrate/visual/`) call external
language and vision models. Until now the shipped default was a direct OpenAI
endpoint with `OPENAI_API_KEY`, and an Anthropic backend with
`ANTHROPIC_API_KEY` was also available. The organisation does not permit direct
OpenAI or Anthropic API keys: model access must go through OpenRouter, a local
or self-hosted Ollama, or merge.dev.

## Decision

1. **Allowed providers only.** LLM and vision calls may target:
   - **OpenRouter** — `https://openrouter.ai/api/v1`, key `OPENROUTER_API_KEY`
     (OpenAI-compatible chat completions; model ids are OpenRouter ids such as
     `openai/gpt-4o-mini`).
   - **Ollama** — `http://localhost:11434` or a self-hosted URL, no key.
   - **merge.dev** — allowed host (`api.merge.dev`); no LLM backend uses it
     today, so it is allowlisted but unused.
2. **Direct providers are refused.** `api.openai.com`, `api.anthropic.com`,
   `OPENAI_API_KEY` and `ANTHROPIC_API_KEY` are rejected for LLM/vision use.
3. **Enforced in code, not by convention.**
   `movie_radio_types::llm_policy::check_endpoint(base_url, api_key_env)` is an
   allowlist: unknown hosts are refused, not trusted. It runs in
   `build_narrator_backend`, `OpenRouterVision::from_config` and
   `validate_app_config`, so a bad config fails at load time with a message
   naming ADR-131. Unit tests cover allowed hosts, forbidden hosts, forbidden
   keys, unknown hosts, userinfo/port/case parsing, and a forbidden host hidden
   in a URL path.
4. **Backends removed.** The `openai` and `anthropic` narrator backends and the
   `openai` vision provider are gone; `openrouter` and `ollama_local` /
   `ollama` remain. `config/default.toml`, `.env.example` and
   `schema/config.schema.json` list only the allowed names.
5. **OpenAI-compatible TTS is not a direct-provider path.** The voice provider
   in `crates/movie-radio-voice/src/voice/openai.rs` is kept for explicit local
   sidecars (`OPENAI_TTS_BASE_URL`), but a bare `OPENAI_API_KEY` no longer
   enables it. Removing the provider entirely is tracked in
   `plans/FOLLOWUPS.md`.

## Consequences

- One key (`OPENROUTER_API_KEY`) covers hosted models; Ollama covers offline
  runs; model choice is an OpenRouter model id, not a provider integration.
- A config that points at a direct provider fails fast instead of silently
  spending a forbidden key.
- Adding a provider is a deliberate change: update `ALLOWED_PROVIDERS` /
  `ALLOWED_HOSTS` in `llm_policy.rs`, this ADR, and the tests. Do not add a
  direct provider endpoint.
- The policy covers LLM/vision. TTS providers (ElevenLabs, Modal, audio.cpp,
  Kokoro, Orpheus, Qwen3, the OpenAI-compatible sidecar) are separate and are
  not restricted by this ADR.

## References

- `crates/movie-radio-types/src/llm_policy.rs`
- `crates/movie-radio-pipeline/src/narrator.rs`
- `crates/movie-radio-goap/src/narrate/visual/provider.rs`
- ADR-126 (narrator prompt engine), ADR-128 (audio-description standards)
