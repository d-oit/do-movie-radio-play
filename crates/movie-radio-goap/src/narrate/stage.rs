//! Narration stage selection: visual, LLM-backed or deterministic acoustic.
//!
//! Kept out of `actions.rs` so the action stays a thin GOAP step and every
//! path returns the same report shape for the caller to persist.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use movie_radio_pipeline::narrator::build_narrator_backend;
use movie_radio_types::{
    parse_narrator_style, NarratorConfig, NarratorParams, TimelineOutput, VisualGap,
};

use super::llm::{self, LlmNarrationReport, LlmNarrator, ProviderBackend, PROMPT_VERSION};
use super::visual::{VisualNarrationReport, VisualNarrator};
use super::{NarrationEvidence, NarrationGenerator, NarrationScript};

pub struct NarrationStageOutcome {
    pub scripts: Vec<NarrationScript>,
    pub visual_report: Option<VisualNarrationReport>,
    pub llm_report: Option<LlmNarrationReport>,
}

pub async fn generate(
    generator: &NarrationGenerator,
    config: &NarratorConfig,
    known_characters: &[String],
    movie_path: &Path,
    timeline: &TimelineOutput,
    gaps: &[VisualGap],
) -> Result<NarrationStageOutcome> {
    if config.visual.provider != "none" {
        return generate_visual(
            generator,
            config,
            known_characters,
            movie_path,
            timeline,
            gaps,
        )
        .await;
    }
    if config.llm.enabled {
        return generate_llm(generator, config, timeline, gaps).await;
    }
    Ok(NarrationStageOutcome {
        scripts: generator.generate(timeline, gaps)?,
        visual_report: None,
        llm_report: None,
    })
}

async fn generate_visual(
    generator: &NarrationGenerator,
    config: &NarratorConfig,
    known_characters: &[String],
    movie_path: &Path,
    timeline: &TimelineOutput,
    gaps: &[VisualGap],
) -> Result<NarrationStageOutcome> {
    let narrator = VisualNarrator::new(config.visual.clone(), &generator.language)
        .with_known_characters(known_characters.to_vec());
    match narrator.generate(movie_path, timeline, gaps).await {
        Ok(outcome) => Ok(NarrationStageOutcome {
            scripts: outcome.scripts,
            visual_report: Some(outcome.report),
            llm_report: None,
        }),
        Err(error) => {
            tracing::warn!(
                error = %error,
                "visual narration stage failed; using labelled acoustic fallback"
            );
            let reason = format!("visual stage failed: {error}");
            Ok(NarrationStageOutcome {
                scripts: labelled_fallback(generator, timeline, gaps, &reason)?,
                visual_report: None,
                llm_report: None,
            })
        }
    }
}

async fn generate_llm(
    generator: &NarrationGenerator,
    config: &NarratorConfig,
    timeline: &TimelineOutput,
    gaps: &[VisualGap],
) -> Result<NarrationStageOutcome> {
    let model = configured_model(config);
    let backend = match build_narrator_backend(config) {
        Ok(backend) => backend,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "LLM narration backend unavailable; using labelled acoustic fallback"
            );
            let reason = format!("llm backend unavailable: {error}");
            let scripts = labelled_fallback(generator, timeline, gaps, &reason)?;
            return Ok(NarrationStageOutcome {
                llm_report: Some(LlmNarrationReport {
                    prompt_version: PROMPT_VERSION.to_string(),
                    provider: config.backend.clone(),
                    model,
                    accepted: 0,
                    rejected: 0,
                    fallback: scripts.len(),
                    windows: Vec::new(),
                }),
                scripts,
                visual_report: None,
            });
        }
    };
    let params = narrator_params(config, &generator.language);
    let provider = Arc::new(ProviderBackend::new(backend, &config.backend, &model));
    let narrator = LlmNarrator::new(provider, config.llm.max_attempts);
    let outcome =
        llm::generate_scripts(generator, &narrator, &config.style, &params, timeline, gaps).await?;
    Ok(NarrationStageOutcome {
        scripts: outcome.scripts,
        visual_report: None,
        llm_report: Some(outcome.report),
    })
}

fn narrator_params(config: &NarratorConfig, language: &str) -> NarratorParams {
    NarratorParams {
        language: language.to_string(),
        style: parse_narrator_style(&config.style),
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        model: None,
        system_prompt_override: None,
    }
}

fn configured_model(config: &NarratorConfig) -> String {
    match config.backend.as_str() {
        "openrouter" => config.openrouter.model.clone(),
        "ollama_local" => config.ollama_local.model.clone(),
        _ => String::new(),
    }
}

fn labelled_fallback(
    generator: &NarrationGenerator,
    timeline: &TimelineOutput,
    gaps: &[VisualGap],
    reason: &str,
) -> Result<Vec<NarrationScript>> {
    let mut scripts = generator.generate(timeline, gaps)?;
    for script in &mut scripts {
        script.evidence = NarrationEvidence::AudioOnly {
            reason: reason.to_string(),
        };
    }
    Ok(scripts)
}
