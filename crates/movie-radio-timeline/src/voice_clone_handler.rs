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

/// Inventory stored references, surfacing per-file failures instead of
/// silently dropping them: a directory holding only a corrupted JSON file
/// must not be reported as "(none stored yet)". Directory-open failure
/// still propagates; per-file read/parse failures are returned as
/// `(path, reason)` pairs so the caller can warn without discarding the
/// readable entries alongside them.
type VoiceInventory = (Vec<VoiceReference>, Vec<(PathBuf, String)>);

fn collect_voice_references(base_dir: &std::path::Path) -> Result<VoiceInventory> {
    let mut references = Vec::new();
    let mut skipped = Vec::new();
    let entries = fs::read_dir(base_dir).context("failed to read voice_samples directory")?;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                skipped.push((base_dir.to_path_buf(), err.to_string()));
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                skipped.push((path, err.to_string()));
                continue;
            }
        };
        match serde_json::from_str::<Vec<VoiceReference>>(&content) {
            Ok(cands) => references.extend(cands),
            Err(err) => skipped.push((path, err.to_string())),
        }
    }
    Ok((references, skipped))
}

pub fn handle_voice_list() -> Result<()> {
    let base_dir = PathBuf::from("voice_samples");
    if !base_dir.exists() {
        println!("voice references: (none stored yet) — use `voice samples --character NAME --input movie.mkv`");
        Ok(())
    } else {
        let (references, skipped) = collect_voice_references(&base_dir)?;
        for (path, reason) in &skipped {
            eprintln!(
                "warning: skipping unreadable voice samples {}: {reason}",
                path.display()
            );
        }

        if references.is_empty() {
            if skipped.is_empty() {
                println!("voice references: (none stored yet) — use `voice samples --character NAME --input movie.mkv`");
            } else {
                println!(
                    "voice references: (none readable — {} file(s) skipped, see warnings) — repair or re-run `voice samples --character NAME --input movie.mkv`",
                    skipped.len()
                );
            }
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
            if !skipped.is_empty() {
                println!(
                    "warning: skipped {} unreadable file(s) — listing may be incomplete",
                    skipped.len()
                );
            }
        }
        Ok(())
    }
}

/// Load the first usable clone reference from a `voice samples` JSON file.
///
/// Every failure is fatal: a clone test without a clone reference silently
/// degrades to plain synthesis, so stale entries are skipped and an empty,
/// malformed, or reference-less file errors with a recovery hint.
fn load_clone_reference(sample_file: &std::path::Path, character: &str) -> Result<PathBuf> {
    let content = fs::read_to_string(sample_file).with_context(|| {
        format!(
            "no voice samples for character '{character}': cannot read {} — run `voice samples --character {character} --input <movie>` first or pass --samples-from <file>",
            sample_file.display()
        )
    })?;
    let cands: Vec<VoiceReference> = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse voice samples {}", sample_file.display()))?;
    if cands.is_empty() {
        anyhow::bail!(
            "voice samples {} contain no candidates — re-run `voice samples --character {character} --input <movie>`",
            sample_file.display()
        );
    }
    // First candidate whose stored reference still points at a real file;
    // stale entries (moved/deleted clips) are skipped, and a file with no
    // usable reference at all fails instead of testing the wrong voice.
    // Candidates are scoped to the requested character: `--samples-from`
    // accepts an arbitrary file, so without this check
    // `--character alice --samples-from bob.json` would clone Bob's voice
    // while reporting Alice.
    let matching: Vec<&VoiceReference> = cands
        .iter()
        .filter(|c| c.validate().is_ok() && c.character_name == character)
        .collect();
    if !cands.iter().any(|c| c.character_name == character) {
        anyhow::bail!(
            "voice samples {} contain no candidates for character '{character}' — re-run `voice samples --character {character} --input <movie>`",
            sample_file.display()
        );
    }
    matching
        .iter()
        .flat_map(|c| c.sample_paths.iter())
        .find(|p| p.is_file())
        .cloned()
        .with_context(|| {
            format!(
                "voice samples {} contain no usable reference audio for character '{character}' (all sample paths missing or stale) — re-run `voice samples --character {character} --input <movie>`",
                sample_file.display()
            )
        })
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
    let ref_audio = load_clone_reference(&sample_file, &character)?;

    // Actually exercise the clone: build a request and synthesize, so a
    // bad reference or broken provider fails loudly instead of printing.
    let request = movie_radio_voice::SynthesisRequest {
        text: text.clone(),
        emotion: movie_radio_voice::Emotion::Neutral,
        voice_id: None,
        reference_audio: Some(ref_audio.clone()),
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
    // The clone section wins over the general-TTS audio_cpp section: mode,
    // GPU routing prefs, and family/model/language all come from
    // `voice_clone` so a clone-only override reaches the provider even
    // when the shared section still says `auto`/`local`. Transport-level
    // fields (endpoints, auth, backend, cost caps) only exist on the
    // shared section and are copied across.
    let clone_mode = if cfg.voice_clone.routing.mode == "auto" {
        cfg.voice.audio_cpp.mode.clone()
    } else {
        cfg.voice_clone.routing.mode.clone()
    };
    let audio_cpp = movie_radio_voice::AudioCppConfig {
        enabled: cfg.voice.audio_cpp.enabled,
        mode: clone_mode.clone(),
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
    };
    let voice_cfg = movie_radio_voice::VoiceSynthesisConfig {
        provider: "audio_cpp".to_string(),
        fallback_chain: vec!["audio_cpp".to_string()],
        language: cfg.voice_clone.language.clone(),
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

    // Report the selected routing mode, not URL presence: the provider
    // reaches remote GPU pools when mode is `remote` (and may fall back
    // to them in `auto`), even with an empty `remote.server_url`.
    println!(
        "voice test character={character} text={text:?} runtime={} mode={clone_mode} provider={provider} reference_audio={} samples={} sample_rate_hz={}",
        cfg.voice_clone.runtime,
        ref_audio.display(),
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

    fn write_reference_file(dir: &std::path::Path, paths: &[PathBuf]) -> PathBuf {
        let file = dir.join("refs.json");
        let refs = vec![VoiceReference {
            id: "alice_candidate_1".to_string(),
            character_name: "alice".to_string(),
            sample_paths: paths.to_vec(),
            metadata: std::collections::HashMap::new(),
            created_at: None,
            runtime: "audio_cpp".to_string(),
            family: "qwen3_tts".to_string(),
            model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
            language: "de".to_string(),
        }];
        fs::write(&file, serde_json::to_string_pretty(&refs).unwrap()).unwrap();
        file
    }

    #[test]
    fn missing_samples_file_fails() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("missing.json");
        assert!(load_clone_reference(&missing, "alice").is_err());
    }

    #[test]
    fn corrupted_sample_file_is_surfaced_not_silent() -> Result<()> {
        let dir = TempDir::new()?;
        fs::write(dir.path().join("broken.json"), b"{ not json")?;
        let (refs, skipped) = collect_voice_references(dir.path())?;
        assert!(refs.is_empty());
        assert_eq!(skipped.len(), 1);
        Ok(())
    }

    #[test]
    fn readable_entries_survive_beside_corrupted_file() -> Result<()> {
        let dir = TempDir::new()?;
        fs::write(dir.path().join("broken.json"), b"{ not json")?;
        write_reference_file(dir.path(), &[PathBuf::from("voice_samples/gone.wav")]);
        let (refs, skipped) = collect_voice_references(dir.path())?;
        assert_eq!(refs.len(), 1);
        assert_eq!(skipped.len(), 1);
        Ok(())
    }

    #[test]
    fn other_character_reference_rejected() -> Result<()> {
        let dir = TempDir::new()?;
        let live = dir.path().join("bob.wav");
        fs::write(&live, b"RIFF")?;
        let file = write_reference_file(dir.path(), &[live]);
        // Fixture helper stamps candidates as alice's; loading for bob must
        // fail rather than clone alice's voice, and vice versa.
        assert!(load_clone_reference(&file, "bob").is_err());
        Ok(())
    }

    #[test]
    fn stale_reference_fails_instead_of_plain_synthesis() -> Result<()> {
        let dir = TempDir::new()?;
        let file = write_reference_file(dir.path(), &[PathBuf::from("voice_samples/gone.wav")]);
        assert!(load_clone_reference(&file, "alice").is_err());
        Ok(())
    }

    #[test]
    fn skips_stale_entry_for_live_reference() -> Result<()> {
        let dir = TempDir::new()?;
        let live = dir.path().join("live.wav");
        fs::write(&live, b"RIFF")?;
        let file = dir.path().join("refs.json");
        let refs = vec![
            VoiceReference {
                id: "alice_candidate_1".to_string(),
                character_name: "alice".to_string(),
                sample_paths: vec![PathBuf::from("voice_samples/gone.wav")],
                metadata: std::collections::HashMap::new(),
                created_at: None,
                runtime: "audio_cpp".to_string(),
                family: "qwen3_tts".to_string(),
                model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
                language: "de".to_string(),
            },
            VoiceReference {
                id: "alice_candidate_2".to_string(),
                character_name: "alice".to_string(),
                sample_paths: vec![live.clone()],
                metadata: std::collections::HashMap::new(),
                created_at: None,
                runtime: "audio_cpp".to_string(),
                family: "qwen3_tts".to_string(),
                model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
                language: "de".to_string(),
            },
        ];
        fs::write(&file, serde_json::to_string_pretty(&refs)?)?;
        assert_eq!(load_clone_reference(&file, "alice")?, live);
        Ok(())
    }
}
