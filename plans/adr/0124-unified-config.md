# ADR-124: Unified Config Schema & Layered Loading

**Status**: Accepted
**Date**: 2026-09-01
**Issues**: #241

## Context

Config fragmented: `config/default.json` for analysis, env overrides scattered in `timeline/src/config.rs`, `.env.example` mixed prefixes. #241 requires CLI > env > local.toml > default.toml precedence, capability-neutral schema, JSON Schema, `movie-radio config --validate`.

## Decision

- Canonical `AppConfig` covering `providers`, `voice`, `voice_clone`, `narrator`, `pipeline`, `characters`, `output` without XTTS/Coqui-specific core fields.
- `config` crate (config-rs) layered builder: `set_default` < `File(config/default.toml)` < `File(config/local.toml).required(false)` < `Environment::with_prefix("MRPLAY").separator("__")` (+ legacy `AUDIO_CPP_*` compat). `dotenvy` loads `.env` before builder.
- Preserve existing `AnalysisConfig` via `#[serde(flatten)]` shim for backward compat; `AppConfig::from_analysis` bridge.
- Validation: TOML schema, enums (`auto|local|remote`, `server|cli`, `best|cpu|cuda|vulkan|metal`), URLs (https for remote), `auth_env` existence, `cost_per_job/day`, model/family consistency via allowlist.
- Generate `schema/config.schema.json` via `schemars` (or hand-written + test) and `config --validate` CLI.
- `.env.example` exposes stable high-level controls with `MRPLAY_*` prefix; secrets never printed.

## Consequences

- Single config surface for TTS/voice_clone/SFX/narrator.
- Audio.cpp remains canonical TTS runtime; XTTS legacy envs (`MRPLAY_XTTS_MODEL_PATH`, `backend=xtts_local`) removed.
- Deterministic, validated, secret-safe.
