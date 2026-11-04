# ADR-126: Narrator AI Prompt Engine

**Status**: Accepted
**Date**: 2026-09-01
**Issues**: #240
**Update 2026-11-04**: the `openai` and `anthropic` backends were removed. Backends are now
`openrouter` (OpenRouter, `OPENROUTER_API_KEY`) and `ollama_local`; direct provider endpoints and
keys are rejected by `movie_radio_types::llm_policy` — see ADR-131.

## Context

Narrator must produce radio-drama bridging text with configurable style, language, backend, and hot-reloadable template without code changes.

## Decision

- Trait `NarratorAiBackend: RadioPlugin` with `generate_narration(prompt: &RenderedPrompt, params: &NarratorParams)`.
- Implementations: `OpenAiNarrator`, `OllamaLocalNarrator`, `AnthropicNarrator` via `reqwest` with timeouts, token redaction.
- Template engine **Tera** (Jinja2 compat) for `templates/narrator_prompt.md` with frontmatter `style, language, max_tokens` and variables `movie_title, prev_scene, scene_type, duration_secs, visual_description, characters, mood, max_words`. Runtime `Tera::new` + explicit reload flag/CLI `--reload-templates`; optional file watcher deferred.
- Config `config/default.toml [narrator]` with `backend, language, style, max_tokens, temperature, prompt_template` and per-backend sections; language propagates to prompt + TTS.
- CLI `movie-radio narrate --scene 12 --dry-run` prints rendered text.

## Consequences

- Hot-configurable style/language/backend without recompilation.
- Deterministic rendering tests via mock LLM; streaming not required for v1.
- Secrets via `api_key_env` only; never logged.
