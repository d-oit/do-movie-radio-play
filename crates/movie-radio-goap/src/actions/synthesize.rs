//! `synthesize_narrator`: turns narration scripts into audio through the
//! configured voice chain.

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

use crate::{Action, PipelineContext, WorldState};

#[derive(Debug, Default)]
pub struct SynthesizeNarrator;

#[async_trait]
impl Action for SynthesizeNarrator {
    fn name(&self) -> &str {
        "synthesize_narrator"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            narration_scripts_generated: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            narrator_voice_synthesized: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        5.0
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        use movie_radio_voice::voice::{SynthesisOrchestrator, SynthesisRequest};

        let scripts = ctx.scripts.as_ref().context("Scripts not generated")?;
        // Keep the pre-action length so a failed attempt can roll its partial
        // `None`/audio entries back: the orchestrator retries failed actions and
        // AssembleRadioPlay zips scripts against this vector from its start.
        let narration_baseline = ctx.narration_audio.len();

        let voice_cfg = ctx
            .voice_config
            .clone()
            .unwrap_or_else(movie_radio_voice::config::VoiceSynthesisConfig::from_env);

        let language = voice_cfg.language.clone();
        let voice_id = voice_cfg.voice_id.clone();
        let emotion_mapping = voice_cfg.emotion_mapping;
        let orchestrator = match ctx.voice_orchestrator.clone() {
            Some(prebuilt) => prebuilt,
            None => std::sync::Arc::new(SynthesisOrchestrator::new(voice_cfg)),
        };

        for (i, script) in scripts.iter().enumerate() {
            let emotion = if emotion_mapping {
                script.emotion.clone()
            } else {
                movie_radio_voice::Emotion::Neutral
            };
            info!(
                i = i + 1,
                total = scripts.len(),
                text = %script.text,
                emotion = ?emotion,
                "Synthesizing narration"
            );

            let request = SynthesisRequest {
                text: script.text.clone(),
                emotion,
                voice_id: voice_id.clone(),
                reference_audio: ctx.voice_reference.clone(),
                language: language.clone(),
                // Base speed only: providers resolve `emotion` into their own speed/stability levers.
                speed: 1.0,
                sample_rate_hz: ctx.sample_rate,
            };

            if let Err(e) = request.validate() {
                tracing::warn!(i = i + 1, error = %e, "Invalid synthesis request, skipping");
                ctx.narration_audio.push(None);
                ctx.narration_provider.push(None);
                continue;
            }

            match orchestrator.synthesize_with_provider(&request).await {
                Ok((audio, provider_id)) => {
                    info!(
                        i = i + 1,
                        samples = audio.samples.len(),
                        "Narration synthesized"
                    );
                    ctx.narration_audio.push(Some(audio));
                    ctx.narration_provider.push(Some(provider_id));
                }
                Err(e) => {
                    tracing::warn!(i = i + 1, error = %e, "TTS failed, skipping");
                    ctx.narration_audio.push(None);
                    ctx.narration_provider.push(None);
                }
            }
        }

        if !scripts.is_empty() && ctx.narration_audio.iter().all(Option::is_none) {
            ctx.narration_audio.truncate(narration_baseline);
            ctx.narration_provider.truncate(narration_baseline);
            anyhow::bail!(
                "all {} narration syntheses failed; check TTS provider configuration \
                 (e.g. OPENAI_TTS_BASE_URL / MODAL_TTS_ENDPOINT / ELEVENLABS_API_KEY)",
                scripts.len()
            );
        }

        info!(
            count = ctx.narration_audio.iter().filter(|a| a.is_some()).count(),
            total = scripts.len(),
            "Narration synthesis complete"
        );
        Ok(())
    }
}
