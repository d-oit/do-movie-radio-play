use anyhow::{bail, Result};
use movie_radio_types::AppConfig;
use std::path::{Path, PathBuf};

/// Planned `produce` stages, in execution order.
///
/// Stage executors are not implemented yet: the original v1 scaffold wrote an
/// empty checkpoint JSON per stage and reported "produce complete" without
/// running anything. Until executors land (see plans/140-codebase-gap-analysis.md
/// A4), the real run fails loudly and only `--dry-run` is available.
const STAGES: &[&str] = &[
    "ExtractAudio",
    "SceneDetect",
    "VoiceActivityDetect",
    "Transcribe",
    "CharacterAssign",
    "VoiceSynthesize",
    "NarratorGenerate",
    "NarratorSynthesize",
    "SfxSelect",
    "SfxFetch",
    "AudioMix",
    "Export",
];

fn reject_traversal(path: &Path, field: &str) -> Result<()> {
    let s = path.to_string_lossy();
    if s.contains("..") {
        anyhow::bail!("{field} must not contain ..");
    }
    Ok(())
}

/// `produce` entry point (orchestrator v1 — planning surface only).
pub fn handle_produce(
    input: PathBuf,
    resume: Option<PathBuf>,
    dry_run: bool,
    cfg: &AppConfig,
) -> Result<()> {
    reject_traversal(&input, "input")?;
    if let Some(ref r) = resume {
        reject_traversal(r, "resume")?;
    }
    if dry_run {
        println!(
            "produce dry-run input={} config_voice_mode={} paid_allowed={} cost_per_job={}",
            input.display(),
            cfg.voice.audio_cpp.mode,
            cfg.voice.gpu_policy.allow_paid,
            cfg.voice.gpu_policy.max_cost_per_job
        );
        for s in STAGES {
            println!(
                "  stage: {s} provider=audio_cpp execution={} (free_preferred={})",
                if cfg.voice.audio_cpp.remote.server_url.is_empty() {
                    "local"
                } else {
                    "remote"
                },
                cfg.voice.gpu_policy.prefer_free
            );
        }
        if let Some(r) = resume {
            println!("resume checkpoint: {}", r.display());
        }
        return Ok(());
    }
    bail!(
        "produce stage executors are not implemented yet: orchestrator v1 is          planning-only. Run with --dry-run to preview the stage plan          (see plans/140-codebase-gap-analysis.md A4)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_deterministic() {
        let cfg = AppConfig::default();
        assert!(handle_produce(PathBuf::from("movie.mkv"), None, true, &cfg).is_ok());
    }

    #[test]
    fn traversal_rejected() {
        let cfg = AppConfig::default();
        assert!(handle_produce(PathBuf::from("../evil.mkv"), None, true, &cfg).is_err());
    }

    #[test]
    fn real_run_bails_until_executors_exist() {
        let cfg = AppConfig::default();
        let err = handle_produce(PathBuf::from("movie.mkv"), None, false, &cfg).unwrap_err();
        assert!(
            err.to_string().contains("not implemented yet"),
            "got: {err}"
        );
    }
}
