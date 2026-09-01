# GOAP State

**Current Goal**: Unified Orchestrator — Implement #237, #241, #239, #240, #238 (Provider & Compute Agnostic + Config + Voice Cloning + Narrator + Pipeline)
**Status**: In-Progress
**Branch**: feat/goap-unified-orchestrator
**Issues**: #237 #241 #239 #240 #238 (depends on closed #235)
**Strategy**: Hybrid — Sequential foundation (config+provider) → Parallel swarm (voice-clone + narrator + orchestrator scaffolding)

## Task Graph
- [ ] T0: Branch & GOAP state init (this file) + workflow-state.json
- [ ] T1: ADRs 123-126 (provider, unified config, voice-clone, narrator)
- [ ] T2: Unified Config Schema (#241) — AppConfig, layered loading (CLI>env>local.toml>default.toml), MRPLAY_* env, validation, JSON schema, .env.example
- [ ] T3: Provider Architecture (#237) — ExecutionLocation, ComputeEndpoint, registry, GPU pool routing (provider-neutral)
- [ ] T4: Voice Cloning Pipeline (#239) — VoiceReference types, sample extraction, persistence, capability checks, routing
- [ ] T5: Narrator AI Prompt Engine (#240) — NarratorAiBackend trait, OpenAI/Ollama/Anthropic, Tera templates, hot-reload, CLI dry-run
- [ ] T6: Full Pipeline Orchestrator (#238) — 11 stages, checkpoint, retry policy, compute-aware scheduling, produce CLI
- [ ] T7: Quality gates (fmt, clippy -p types/pipeline/render/io/voice/goap, tests, quality_gate.sh)
- [ ] T8: Closeout docs & PR

## Evidence Log
- 2026-09-01: Plan approved — audited 5 open issues, verified deps, researched audio.cpp server API, config-rs layered best practice, Tera templating
- Research: audio.cpp server endpoints GET /health /v1/models POST /v1/audio/speech, config crate layered builder, Tera 2.1 runtime templates, dotenvy fork

## History
- 2026-09-01: T0 start — branch feat/goap-unified-orchestrator from main @73ae0c2
