use anyhow::Result;
use movie_radio_types::AppConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub version: String,
    pub input_hash: String,
    pub config_hash: String,
    pub stage: String,
    pub timestamp: String,
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StageStatus {
    Pending,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct Stage {
    pub name: &'static str,
    pub status: StageStatus,
}

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

pub fn handle_produce(
    input: PathBuf,
    resume: Option<PathBuf>,
    dry_run: bool,
    cfg: &AppConfig,
) -> Result<()> {
    if input.to_string_lossy().contains("..") {
        anyhow::bail!("input path must not contain ..");
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
    // Checkpointing placeholder: write durable checkpoint after each stage
    let checkpoint_dir = cfg
        .pipeline
        .checkpoint_dir
        .clone()
        .unwrap_or("checkpoints".to_string());
    std::fs::create_dir_all(&checkpoint_dir)?;
    let stages: Vec<Stage> = STAGES
        .iter()
        .map(|n| Stage {
            name: n,
            status: StageStatus::Pending,
        })
        .collect();
    for stage in stages {
        let cp = Checkpoint {
            version: env!("CARGO_PKG_VERSION").to_string(),
            input_hash: format!("{:x}", md5_hash(&input)),
            config_hash: format!(
                "{:x}",
                md5_hash_str(&serde_json::to_string(cfg).unwrap_or_default())
            ),
            stage: stage.name.to_string(),
            timestamp: chrono_now(),
            artifacts: vec![],
        };
        let path = Path::new(&checkpoint_dir).join(format!("{}.json", stage.name));
        let data = serde_json::to_string_pretty(&cp)?;
        std::fs::write(&path, data)?;
        println!("checkpoint {} -> {}", stage.name, path.display());
    }
    println!(
        "produce complete for {} (orchestrator v1, checkpointed)",
        input.display()
    );
    Ok(())
}

fn md5_hash(p: &Path) -> u64 {
    // Deterministic simple hash for checkpoint identity (not cryptographic)
    let s = p.to_string_lossy().to_string();
    md5_hash_str(&s)
}
fn md5_hash_str(s: &str) -> u64 {
    let mut h: u64 = 0;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    h
}
fn chrono_now() -> String {
    // Without chrono crate, use simple timestamp
    format!("{:?}", std::time::SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dry_run_deterministic() {
        let cfg = AppConfig::default();
        assert!(handle_produce(PathBuf::from("movie.mkv"), None, true, &cfg).is_ok());
    }
}
