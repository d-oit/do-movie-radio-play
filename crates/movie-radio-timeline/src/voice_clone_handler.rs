use anyhow::{Context, Result};
use movie_radio_pipeline::voice_clone::extract_candidates;
use movie_radio_types::VoiceReference;
use std::fs;
use std::path::PathBuf;

/// Filename-safe character handle: rejects path separators and `..` so the
/// handle can back JSON file names and directory lookups.
fn validate_character_handle(character: &str) -> Result<()> {
    if character.trim().is_empty() {
        anyhow::bail!("character must not be empty");
    }
    if character.contains("..") || character.contains(['/', '\\']) {
        anyhow::bail!("character must not contain path separators or ..");
    }
    Ok(())
}

pub fn handle_voice_samples(
    character: String,
    input: PathBuf,
    output: Option<PathBuf>,
) -> Result<()> {
    validate_character_handle(&character)?;

    let cfg = crate::app_config_loader::load_app_config(None)?;
    let candidates = extract_candidates(&input, &cfg, &character)?;

    let out = output.unwrap_or_else(|| PathBuf::from(format!("voice_samples/{character}.json")));

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
    }

    let json_data = serde_json::to_string_pretty(&candidates)
        .context("failed to serialize extracted candidates")?;
    fs::write(&out, json_data)
        .with_context(|| format!("failed to write voice candidates to {}", out.display()))?;

    println!(
        "voice samples for {character} from {} -> {} (extracted {} candidate(s))",
        input.display(),
        out.display(),
        candidates.len()
    );
    println!(
        "runtime={} family={} — capability check passed",
        cfg.voice_clone.runtime, cfg.voice_clone.family
    );
    Ok(())
}

pub fn handle_voice_list() -> Result<()> {
    let base_dir = PathBuf::from("voice_samples");
    if !base_dir.exists() {
        println!("voice references: (none stored yet) — use `voice samples --character NAME --input movie.mkv`");
        Ok(())
    } else {
        let mut references = Vec::new();
        let entries = fs::read_dir(&base_dir).context("failed to read voice_samples directory")?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(cands) = serde_json::from_str::<Vec<VoiceReference>>(&content) {
                        references.extend(cands);
                    }
                }
            }
        }

        if references.is_empty() {
            println!("voice references: (none stored yet) — use `voice samples --character NAME --input movie.mkv`");
        } else {
            println!("voice references (stored total: {}):", references.len());
            for r in &references {
                println!(
                    " - [{}] character={} runtime={} family={} samples={}",
                    r.id,
                    r.character_name,
                    r.runtime,
                    r.family,
                    r.sample_paths.len()
                );
            }
        }
        Ok(())
    }
}

pub fn handle_voice_test(
    character: String,
    text: String,
    samples_from: Option<PathBuf>,
) -> Result<()> {
    validate_character_handle(&character)?;
    let cfg = crate::app_config_loader::load_app_config(None)?;

    let sample_file =
        samples_from.unwrap_or_else(|| PathBuf::from(format!("voice_samples/{character}.json")));
    let mut ref_audio = None;

    if sample_file.exists() {
        if let Ok(content) = fs::read_to_string(&sample_file) {
            if let Ok(cands) = serde_json::from_str::<Vec<VoiceReference>>(&content) {
                if let Some(first) = cands.first() {
                    // Only surface a stored reference when it still points at a
                    // real file; stale/mock paths stay local-only.
                    if let Some(path) = first.sample_paths.first() {
                        if path.exists() {
                            ref_audio = Some(path.clone());
                        }
                    }
                }
            }
        }
    }

    // Actually exercise the clone: build a request and synthesize, so a
    // bad reference or broken provider fails loudly instead of printing.
    let request = movie_radio_voice::SynthesisRequest {
        text: text.clone(),
        emotion: movie_radio_voice::Emotion::Neutral,
        voice_id: None,
        reference_audio: ref_audio.clone(),
        language: cfg.voice_clone.language.clone(),
        speed: 1.0,
        sample_rate_hz: 16_000,
    };
    request.validate().map_err(|e| anyhow::anyhow!(e))?;
    // Synthesize with the loaded config's audio.cpp section: `from_env()`
    // defaults to the modal chain and never adds the configured audio_cpp
    // provider, while the repo default selects audio_cpp for cloning.
    // Remote auth and GPU pool ride along so a configured authenticated
    // or pooled remote endpoint is actually exercised.
    //
    // NOTE: `movie_radio_types::config::AudioCppConfig` and
    // `movie_radio_voice::config::AudioCppConfig` are distinct types with
    // the same shape; the fields are copied across here.
    // Voice-clone routing wins over the general-TTS mode: a clone-only
    // `remote` routing must reach the provider even when the shared
    // audio_cpp section still says `auto`/`local`.
    let clone_mode = if cfg.voice_clone.routing.mode == "auto" {
        cfg.voice.audio_cpp.mode.clone()
    } else {
        cfg.voice_clone.routing.mode.clone()
    };
    let audio_cpp = movie_radio_voice::AudioCppConfig {
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
        family: cfg.voice.audio_cpp.family.clone(),
        model: cfg.voice.audio_cpp.model.clone(),
        backend: cfg.voice.audio_cpp.backend.clone(),
        language: cfg.voice.audio_cpp.language.clone(),
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
            prefer_free: cfg.voice.gpu_policy.prefer_free,
            allow_paid: cfg.voice.gpu_policy.allow_paid,
            max_cost_per_job: cfg.voice.gpu_policy.max_cost_per_job,
            max_cost_per_day: cfg.voice.gpu_policy.max_cost_per_day,
        },
    };
    let voice_cfg = movie_radio_voice::VoiceSynthesisConfig {
        provider: "audio_cpp".to_string(),
        fallback_chain: vec!["audio_cpp".to_string()],
        language: cfg.voice.audio_cpp.language.clone(),
        voice_id: cfg.voice.audio_cpp.voice_id.clone(),
        providers: movie_radio_voice::VoiceProvidersConfig {
            audio_cpp: Some(audio_cpp),
            ..movie_radio_voice::VoiceProvidersConfig::default()
        },
        ..movie_radio_voice::VoiceSynthesisConfig::default()
    };
    let orchestrator = movie_radio_voice::voice::SynthesisOrchestrator::new(voice_cfg);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create async runtime for voice test")?;
    let (audio, provider) = rt.block_on(orchestrator.synthesize_with_provider(&request))?;

    println!(
        "voice test character={character} text={text:?} runtime={} endpoint={} provider={provider} reference_audio={ref_audio:?} samples={} sample_rate_hz={}",
        cfg.voice_clone.runtime,
        if cfg.voice.audio_cpp.remote.server_url.is_empty() { "local" } else { "remote" },
        audio.samples.len(),
        audio.sample_rate_hz,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_handle_voice_samples_and_list() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let out_json = temp_dir.path().join("alice.json");

        handle_voice_samples(
            "alice".to_string(),
            PathBuf::from("testdata/movie.mkv"),
            Some(out_json.clone()),
        )?;
        assert!(out_json.exists());

        let content = fs::read_to_string(&out_json)?;
        let cands: Vec<VoiceReference> = serde_json::from_str(&content)?;
        assert!(!cands.is_empty());
        assert_eq!(cands[0].character_name, "alice");

        Ok(())
    }

    #[test]
    fn test_traversal_character_rejected() {
        assert!(validate_character_handle("../../pwn").is_err());
        assert!(validate_character_handle("a/b").is_err());
        assert!(validate_character_handle("").is_err());
    }
}
