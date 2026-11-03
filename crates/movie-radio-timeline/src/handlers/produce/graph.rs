//! Input fingerprints and the `--dry-run` execution graph for `produce`.

use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use movie_radio_goap::production::{self, fingerprint, input_class, InputIdentity};
use movie_radio_types::AppConfig;
use movie_radio_voice::config::VoiceSynthesisConfig;

use super::{goal_state, ProduceOptions};

/// Per-input-class hashes, so a stage is reused while the classes it does not
/// read stay unchanged.
pub(super) fn run_fingerprints(
    opts: &ProduceOptions,
    input: &InputIdentity,
    voice: &VoiceSynthesisConfig,
    deliverable: &Path,
    cues: Option<&Path>,
    cfg: &AppConfig,
    analysis: &movie_radio_types::AnalysisConfig,
) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    let source = format!(
        "{}|{}|{}",
        input.path, input.size_bytes, input.modified_unix_ms
    );
    map.insert(
        input_class::SOURCE.to_string(),
        fingerprint(&[("source", source)]),
    );
    let cues_label = match cues {
        Some(path) => match fs::metadata(path) {
            Ok(meta) => format!("{}|{}", path.display(), meta.len()),
            Err(_) => format!("{}|missing", path.display()),
        },
        None => "none".to_string(),
    };
    map.insert(
        input_class::CUES.to_string(),
        fingerprint(&[("cues", cues_label)]),
    );
    map.insert(
        input_class::ANALYSIS.to_string(),
        fingerprint(&[("analysis", analysis_label(analysis))]),
    );
    map.insert(
        input_class::GAPS.to_string(),
        fingerprint(&[(
            "gaps",
            format!(
                "confidence={:?} no_auto_cues={}",
                opts.gap_confidence, opts.no_auto_cues
            ),
        )]),
    );
    map.insert(
        input_class::NARRATION.to_string(),
        fingerprint(&[("narration", format!("lang={}", voice.language))]),
    );
    map.insert(
        input_class::VOICE.to_string(),
        fingerprint(&[(
            "voice",
            format!(
                "provider={} chain={} voice_id={:?} lang={} emotion={}",
                voice.provider,
                voice.fallback_chain.join(","),
                voice.voice_id,
                voice.language,
                voice.emotion_mapping
            ),
        )]),
    );
    map.insert(
        input_class::ASSEMBLY.to_string(),
        fingerprint(&[(
            "assembly",
            format!(
                "out={}|sfx={}|mastering={}",
                deliverable.display(),
                serde_json::to_string(&cfg.sound_effects)?,
                opts.mastering_profile
                    .as_deref()
                    .unwrap_or(movie_radio_render::mastering::DEFAULT_PROFILE)
            ),
        )]),
    );
    map.insert(
        input_class::VERIFICATION.to_string(),
        fingerprint(&[(
            "verification",
            format!(
                "verify={} mastering={}",
                opts.verify_quality,
                opts.mastering_profile
                    .as_deref()
                    .unwrap_or(movie_radio_render::mastering::DEFAULT_PROFILE)
            ),
        )]),
    );
    Ok(map)
}

fn analysis_label(cfg: &movie_radio_types::AnalysisConfig) -> String {
    format!(
        "sr={} frame={} hangover={} merge={} min_speech={} min_non_voice={} \
         max_non_voice={:?} energy={} delta={} engine={}",
        cfg.sample_rate_hz,
        cfg.frame_ms,
        cfg.speech_hangover_ms,
        cfg.merge_gap_ms,
        cfg.min_speech_ms,
        cfg.min_non_voice_ms,
        cfg.max_non_voice_ms,
        cfg.energy_threshold,
        cfg.vad_threshold_delta,
        cfg.vad_engine
    )
}

fn credential_state(provider: &str) -> String {
    match PROVIDER_CREDENTIALS.iter().find(|(id, _)| *id == provider) {
        Some((_, env)) => format!(
            "env {env} {}",
            if std::env::var_os(env).is_some() {
                "present"
            } else {
                "MISSING"
            }
        ),
        None => "no credential mapping".to_string(),
    }
}

pub(super) fn print_graph(
    opts: &ProduceOptions,
    run_dir: &Path,
    checkpoint_path: &Path,
    deliverable: &Path,
    fingerprints: &BTreeMap<String, String>,
    voice: &VoiceSynthesisConfig,
) {
    let goal = goal_state(opts);
    let required = production::required_stages(&goal);
    println!("produce dry-run (full radio-play production)");
    println!("  input:        {}", opts.input.display());
    println!("  run dir:      {}", run_dir.display());
    println!("  checkpoint:   {}", checkpoint_path.display());
    println!("  deliverable:  {}", deliverable.display());
    println!(
        "  voice chain:  {} (language={} voice_id={})",
        voice.fallback_chain.join(" -> "),
        voice.language,
        voice.voice_id.as_deref().unwrap_or("<provider default>")
    );
    let mut credentialed = false;
    for provider in &voice.fallback_chain {
        let state = credential_state(provider);
        credentialed |= state.ends_with("present");
        println!("    {provider}: {state}");
    }
    if !credentialed {
        println!(
            "    WARNING: no voice provider in the chain has credentials; \
             synthesis will fail instead of shipping a silent deliverable"
        );
    }
    println!("  input classes:");
    for (class, hash) in fingerprints {
        println!("    {class:<13} {hash}");
    }
    println!("  stages ({} required):", required.len());
    for (i, stage) in required.iter().enumerate() {
        println!(
            "    {}/{} {:<20} action={} inputs={} artifacts={}",
            i + 1,
            required.len(),
            stage.name,
            stage.action,
            stage.inputs.join(","),
            stage.produces
        );
        println!("        requirements: {}", stage.requirements);
    }
    for stage in production::STAGES {
        if !required.iter().any(|s| s.name == stage.name) {
            println!("    skipped: {} (goal flag not set)", stage.name);
        }
    }
    println!(
        "  external work: one TTS request per narration script, to the first \
         provider in the chain that has credentials"
    );
}

/// Provider id → environment variable that must be set for it to serve work.
const PROVIDER_CREDENTIALS: &[(&str, &str)] = &[
    ("modal", movie_radio_voice::config::ENV_MODAL_TTS_ENDPOINT),
    (
        "elevenlabs",
        movie_radio_voice::config::ENV_ELEVENLABS_API_KEY,
    ),
    ("openai", movie_radio_voice::config::ENV_OPENAI_API_KEY),
    ("kokoro", movie_radio_voice::config::ENV_KOKORO_ENDPOINT_URL),
];
