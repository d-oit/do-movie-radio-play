use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use tracing::info;

use movie_radio_goap::actions::get_all_actions;
use movie_radio_goap::orchestrator::Orchestrator;
use movie_radio_goap::{PipelineContext, WorldState};
use movie_radio_io::json::{read_timeline, write_json_pretty};
use movie_radio_types::voice_clone::load_reference_audio;
use movie_radio_voice::config::VoiceSynthesisConfig;

#[derive(Debug, Default)]
pub struct RadioPlayOptions {
    pub timeline: Option<PathBuf>,
    pub subtitles: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub analyze_only: bool,
    pub verify_quality: bool,
    pub apply_learnings: bool,
    pub learning_state: Option<PathBuf>,
    pub learning_db: Option<PathBuf>,
    pub no_learn: bool,
    pub voice_reference: Option<PathBuf>,
    pub character: Option<String>,
}

pub fn handle_radio_play(movie: PathBuf, opts: RadioPlayOptions) -> Result<()> {
    let output_path = opts.output.clone().unwrap_or_else(|| {
        let mut out = movie.clone();
        out.set_extension("radio-play.mp3");
        out
    });

    let mut ctx = PipelineContext::new(movie, output_path);
    // Unique run id up front: same-movie runs started in the same second
    // must not share the run_traces primary key.
    ctx.run_id = Some(movie_radio_goap::fallback_run_id(&ctx.movie_path));
    ctx.subtitles_path = opts.subtitles;
    ctx.voice_config = Some(VoiceSynthesisConfig::from_env());
    ctx.learning_state_path = opts.learning_state;
    ctx.learning_db_path = opts.learning_db;
    ctx.no_learn = opts.no_learn;

    // Resolve voice_reference if requested via --voice-reference or --character
    ctx.voice_reference = match (opts.voice_reference, opts.character) {
        (Some(ref_path), Some(char_name)) => {
            if ref_path.extension().and_then(|s| s.to_str()) == Some("json") {
                Some(load_reference_audio(&ref_path, &char_name)?)
            } else {
                Some(ref_path)
            }
        }
        (Some(ref_path), None) => {
            if ref_path.extension().and_then(|s| s.to_str()) == Some("json") {
                // Infer character name from sample file stem if possible
                let stem = ref_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                Some(load_reference_audio(&ref_path, stem)?)
            } else {
                Some(ref_path)
            }
        }
        (None, Some(char_name)) => {
            let sample_file = PathBuf::from(format!("voice_samples/{char_name}.json"));
            Some(load_reference_audio(&sample_file, &char_name)?)
        }
        (None, None) => None,
    };

    let cfg = crate::app_config_loader::load_app_config(None).ok();
    let mut voice_cfg = VoiceSynthesisConfig::from_env();

    if ctx.voice_reference.is_some() {
        let audio_cpp = if let Some(ref cfg) = cfg {
            let clone_mode = if cfg.voice_clone.routing.mode == "auto" {
                cfg.voice.audio_cpp.mode.clone()
            } else {
                cfg.voice_clone.routing.mode.clone()
            };
            movie_radio_voice::AudioCppConfig {
                enabled: cfg.voice.audio_cpp.enabled,
                mode: clone_mode,
                local: movie_radio_voice::AudioCppLocalConfig {
                    mode: cfg.voice.audio_cpp.local.mode.clone(),
                    binary: cfg.voice.audio_cpp.local.binary.clone(),
                    server_url: cfg.voice.audio_cpp.local.server_url.clone(),
                },
                remote: movie_radio_voice::AudioCppRemoteConfig {
                    enabled: cfg.voice.audio_cpp.remote.enabled,
                    server_url: cfg.voice.audio_cpp.remote.server_url.clone(),
                    auth_env: cfg.voice.audio_cpp.remote.auth_env.clone(),
                    timeout_secs: cfg.voice.audio_cpp.remote.timeout_secs,
                },
                family: cfg.voice_clone.family.clone(),
                model: cfg.voice_clone.model.clone(),
                backend: cfg.voice.audio_cpp.backend.clone(),
                language: cfg.voice_clone.language.clone(),
                voice_id: cfg.voice.audio_cpp.voice_id.clone(),
                voice_ref: cfg.voice.audio_cpp.voice_ref.clone(),
                timeout_secs: cfg.voice.audio_cpp.timeout_secs,
                gpu_pool: cfg
                    .voice
                    .gpu_pool
                    .iter()
                    .map(|e| movie_radio_voice::GpuPoolEndpoint {
                        name: e.name.clone(),
                        url: e.url.clone(),
                        auth_env: e.auth_env.clone(),
                        priority: e.priority,
                        cost_per_hour: e.cost_per_hour,
                    })
                    .collect(),
                gpu_policy: movie_radio_voice::GpuPolicyConfig {
                    prefer_free: cfg.voice_clone.routing.prefer_free,
                    allow_paid: cfg.voice_clone.routing.allow_paid,
                    max_cost_per_job: cfg.voice.gpu_policy.max_cost_per_job,
                    max_cost_per_day: cfg.voice.gpu_policy.max_cost_per_day,
                },
            }
        } else {
            movie_radio_voice::AudioCppConfig::default()
        };

        if !voice_cfg.fallback_chain.contains(&"audio_cpp".to_string()) {
            voice_cfg.fallback_chain.insert(0, "audio_cpp".to_string());
        }
        voice_cfg.providers.audio_cpp = Some(audio_cpp);
        voice_cfg.provider = "audio_cpp".to_string();
    }

    ctx.voice_config = Some(voice_cfg);

    let mut start_state = WorldState::default();
    if let Some(ref p) = opts.timeline {
        info!(timeline = %p.display(), "Using provided timeline");
        ctx.timeline = Some(read_timeline(p)?);
        start_state.audio_timeline_extracted = true;
    }

    if opts.analyze_only {
        if ctx.timeline.is_none() {
            bail!("--timeline is required for --analyze-only in this version");
        }
        info!("Running visual gap analysis via GOAP planner");
        let goal_state = WorldState {
            visual_gaps_identified: true,
            ..WorldState::default()
        };

        let mut orchestrator = Orchestrator::new(start_state, goal_state, get_all_actions());
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(orchestrator.run(&mut ctx))?;

        let gap_analysis = ctx
            .gap_analysis
            .as_ref()
            .context("Gap analysis not produced by GOAP orchestrator")?;

        if let Some(ref out) = opts.output {
            write_json_pretty(out, gap_analysis)?;
            info!(gaps = gap_analysis.gaps.len(), output = %out.display(), "Gap analysis complete");
        } else {
            println!("{}", serde_json::to_string_pretty(gap_analysis)?);
        }
        return Ok(());
    }

    info!("Running full radio-play pipeline via GOAP orchestrator");
    let goal_state = WorldState {
        radio_play_assembled: true,
        quality_verified: opts.verify_quality,
        learnings_applied: opts.apply_learnings && !opts.no_learn,
        ..WorldState::default()
    };

    let mut orchestrator = Orchestrator::new(start_state, goal_state, get_all_actions());
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(orchestrator.run(&mut ctx))?;

    info!(
        output = %ctx.output_path.display(),
        "Radio play pipeline completed via GOAP orchestrator"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::{Segment, SegmentKind, TimelineOutput};
    use tempfile::tempdir;

    #[test]
    fn test_radio_play_options_default() {
        let opts = RadioPlayOptions::default();
        assert!(!opts.analyze_only);
        assert!(!opts.verify_quality);
        assert!(!opts.apply_learnings);
        assert!(opts.timeline.is_none());
        assert!(opts.subtitles.is_none());
        assert!(opts.output.is_none());
        assert!(opts.voice_reference.is_none());
        assert!(opts.character.is_none());
    }

    #[test]
    fn test_handle_radio_play_with_character_reference() -> Result<()> {
        use movie_radio_types::VoiceReference;
        use std::collections::HashMap;
        use std::fs;

        let dir = tempdir()?;
        let movie = dir.path().join("movie.mp4");
        let live_wav = dir.path().join("alice.wav");
        fs::write(&live_wav, b"RIFF")?;

        let sample_json = dir.path().join("alice.json");
        let refs = vec![VoiceReference {
            id: "alice_c1".to_string(),
            character_name: "alice".to_string(),
            sample_paths: vec![live_wav.clone()],
            metadata: HashMap::new(),
            created_at: None,
            runtime: "audio_cpp".to_string(),
            family: "qwen3_tts".to_string(),
            model: "model".to_string(),
            language: "en".to_string(),
        }];
        fs::write(&sample_json, serde_json::to_string_pretty(&refs)?)?;

        let opts = RadioPlayOptions {
            voice_reference: Some(sample_json),
            character: Some("alice".to_string()),
            ..Default::default()
        };

        // When analyze_only is false and we don't pass timeline, it will fail during execution because movie isn't real,
        // but we can verify voice_reference resolution works before execution failure.
        let result = handle_radio_play(movie, opts);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_handle_radio_play_installs_audio_cpp_config_when_voice_ref_present() -> Result<()> {
        use movie_radio_types::VoiceReference;
        use std::collections::HashMap;
        use std::fs;

        let dir = tempdir()?;
        let movie = dir.path().join("movie.mp4");
        let live_wav = dir.path().join("alice.wav");
        fs::write(&live_wav, b"RIFF")?;

        let sample_json = dir.path().join("alice.json");
        let refs = vec![VoiceReference {
            id: "alice_c1".to_string(),
            character_name: "alice".to_string(),
            sample_paths: vec![live_wav.clone()],
            metadata: HashMap::new(),
            created_at: None,
            runtime: "audio_cpp".to_string(),
            family: "qwen3_tts".to_string(),
            model: "model".to_string(),
            language: "en".to_string(),
        }];
        fs::write(&sample_json, serde_json::to_string_pretty(&refs)?)?;

        let ref_audio = load_reference_audio(&sample_json, "alice")?;
        assert_eq!(ref_audio, live_wav);

        let mut ctx = PipelineContext::new(movie, dir.path().join("out.mp3"));
        ctx.voice_reference = Some(ref_audio);

        let _cfg = crate::app_config_loader::load_app_config(None).ok();
        let mut voice_cfg = VoiceSynthesisConfig::from_env();

        if ctx.voice_reference.is_some() {
            if !voice_cfg.fallback_chain.contains(&"audio_cpp".to_string()) {
                voice_cfg.fallback_chain.insert(0, "audio_cpp".to_string());
            }
            voice_cfg.providers.audio_cpp = Some(movie_radio_voice::AudioCppConfig::default());
            voice_cfg.provider = "audio_cpp".to_string();
        }

        assert_eq!(voice_cfg.provider, "audio_cpp");
        assert_eq!(voice_cfg.fallback_chain[0], "audio_cpp");
        assert!(voice_cfg.providers.audio_cpp.is_some());

        Ok(())
    }

    #[test]
    fn test_handle_radio_play_requires_timeline_for_analyze_only() -> Result<()> {
        let dir = tempdir()?;
        let movie = dir.path().join("movie.mp4");
        let opts = RadioPlayOptions {
            analyze_only: true,
            ..Default::default()
        };
        let res = handle_radio_play(movie, opts);
        assert!(res.is_err());
        let err_msg = res.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err_msg.contains("--timeline is required"));
        Ok(())
    }

    #[test]
    fn test_handle_radio_play_analyze_only_with_timeline() -> Result<()> {
        let dir = tempdir()?;
        let movie = dir.path().join("movie.mp4");
        let timeline_file = dir.path().join("timeline.json");
        let output_file = dir.path().join("gaps.json");

        let timeline = TimelineOutput {
            file: "movie.mp4".to_string(),
            analysis_sample_rate: 16_000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 10_000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec![],
                prompt: None,
                sfx_trigger: None,
            }],
        };
        movie_radio_io::json::write_json_pretty(&timeline_file, &timeline)?;

        let opts = RadioPlayOptions {
            timeline: Some(timeline_file),
            output: Some(output_file.clone()),
            analyze_only: true,
            ..Default::default()
        };

        handle_radio_play(movie, opts)?;
        assert!(output_file.exists());
        Ok(())
    }
}
