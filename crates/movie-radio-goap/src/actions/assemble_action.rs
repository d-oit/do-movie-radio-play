use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use tracing::info;

use crate::assemble::{RadioPlayAssembler, SfxSegment};
use crate::{Action, PipelineContext, WorldState};
use movie_radio_pipeline::pipeline::sfx_autofill::autofill_silent_scene_sfx;
use movie_radio_render::sfx::SfxManager;
use movie_radio_types::SfxTrigger;

#[derive(Debug, Default)]
pub struct AssembleRadioPlay;

#[async_trait]
impl Action for AssembleRadioPlay {
    fn name(&self) -> &str {
        "assemble_radio_play"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            narrator_voice_synthesized: true,
            movie_decoded: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        1.5
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let original = ctx
            .original_audio
            .as_ref()
            .context("Original audio not decoded")?;
        let scripts = ctx.scripts.as_ref().context("Scripts not generated")?;

        let assembler = RadioPlayAssembler::new(ctx.sample_rate, 50, 0.3);
        let narration_segments = assembler.build_narration_segments(scripts, &ctx.narration_audio);

        let mut sfx_segments = Vec::new();
        if let Some(ref mut timeline) = ctx.timeline {
            autofill_silent_scene_sfx(timeline);
            let sfx_cfg = ctx.config.sound_effects.clone().unwrap_or_default();
            if let Ok(sfx_mgr) = SfxManager::from_config(&sfx_cfg) {
                for seg in &timeline.segments {
                    if let Some(ref trigger) = seg.sfx_trigger {
                        if *trigger != SfxTrigger::None {
                            let duration_secs = ((seg.end_ms.saturating_sub(seg.start_ms)) as f32
                                / 1000.0)
                                .clamp(0.0, 300.0);
                            if let Ok(Some(samples)) = sfx_mgr
                                .render_trigger(trigger, ctx.sample_rate, Some(duration_secs))
                                .await
                            {
                                let start_sample = (seg.start_ms as f64 * ctx.sample_rate as f64
                                    / 1000.0)
                                    as usize;
                                sfx_segments.push(SfxSegment {
                                    start_sample,
                                    samples,
                                });
                            }
                        }
                    }
                }
            }
        }

        let radio_play =
            assembler.assemble_with_sfx(original, &narration_segments, &sfx_segments)?;

        if let Some(parent) = ctx.output_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let is_mp3 = ctx
            .output_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("mp3"));

        let target_wav = if is_mp3 {
            ctx.output_path.with_extension("tmp.wav")
        } else {
            ctx.output_path.clone()
        };

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: ctx.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(&target_wav, spec)?;
        for &s in &radio_play {
            let clamped = s.clamp(-1.0, 1.0);
            let sample = (clamped * i16::MAX as f32) as i16;
            writer.write_sample(sample)?;
        }
        writer.finalize()?;

        if is_mp3 {
            encode_to_mp3(&target_wav, &ctx.output_path)?;
            let _ = std::fs::remove_file(&target_wav);
        }

        info!(
            output = %ctx.output_path.display(),
            duration_s = radio_play.len() as f64 / ctx.sample_rate as f64,
            "Radio play assembled"
        );
        ctx.assembled_audio = Some(radio_play);
        Ok(())
    }
}

fn encode_to_mp3(wav_path: &std::path::Path, mp3_path: &std::path::Path) -> Result<()> {
    use std::process::Command;

    let status = Command::new("ffmpeg")
        .arg("-nostdin")
        .arg("-protocol_whitelist")
        .arg("file,pipe,fd")
        .args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(wav_path)
        .args(["-codec:a", "libmp3lame", "-b:a", "192k", "-q:a", "2", "-y"])
        .arg(mp3_path)
        .status()?;

    if !status.success() {
        bail!("ffmpeg MP3 encoding failed with exit code: {}", status);
    }
    Ok(())
}
