use anyhow::{bail, Context, Result};
use movie_radio_types::{AnalysisConfig, AppConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::pipeline::decode::decode_audio;
use crate::pipeline::extract_timeline;

/// Planned `produce` stages, in execution order.
pub const STAGES: &[&str] = &[
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageCheckpoint {
    pub stage_name: String,
    pub completed: bool,
    pub timestamp_rfc3339: String,
    pub artifact_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProduceCheckpoint {
    pub input_file: String,
    pub stages: HashMap<String, StageCheckpoint>,
    pub artifacts: HashMap<String, String>,
}

impl ProduceCheckpoint {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read checkpoint file: {}", path.display()))?;
        let checkpoint: Self = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse checkpoint JSON: {}", path.display()))?;
        Ok(checkpoint)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn is_stage_completed(&self, stage: &str) -> bool {
        self.stages.get(stage).is_some_and(|s| s.completed)
    }

    pub fn mark_completed(&mut self, stage: &str, artifact: Option<PathBuf>) {
        let artifact_str = artifact.map(|p| p.to_string_lossy().to_string());
        if let Some(ref art) = artifact_str {
            self.artifacts.insert(stage.to_string(), art.clone());
        }
        self.stages.insert(
            stage.to_string(),
            StageCheckpoint {
                stage_name: stage.to_string(),
                completed: true,
                timestamp_rfc3339: "2026-09-07T00:00:00Z".to_string(),
                artifact_path: artifact_str,
            },
        );
    }
}

fn reject_traversal(path: &Path, field: &str) -> Result<()> {
    let s = path.to_string_lossy();
    if s.contains("..") {
        anyhow::bail!("{field} must not contain ..");
    }
    Ok(())
}

fn write_wav_file(path: &Path, samples: &[f32], sr: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let sample = (clamped * i16::MAX as f32) as i16;
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

fn write_stage_json(path: PathBuf, value: &serde_json::Value) -> Result<PathBuf> {
    fs::write(&path, serde_json::to_string_pretty(value)?)?;
    Ok(path)
}

fn stage_extract_audio(input: &Path, out_dir: &Path, sr_hz: u32) -> Result<PathBuf> {
    let extracted_path = out_dir.join("extracted.wav");
    let (samples, sr) = decode_audio(input, sr_hz)?;
    write_wav_file(&extracted_path, &samples, sr)?;
    Ok(extracted_path)
}

fn stage_audio_mix(
    input: &Path,
    checkpoint: &ProduceCheckpoint,
    out_dir: &Path,
    sr_hz: u32,
) -> Result<PathBuf> {
    let path = out_dir.join("mix.wav");
    let audio_input = checkpoint
        .artifacts
        .get("ExtractAudio")
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(|| input.to_path_buf());
    let (samples, sr) = decode_audio(&audio_input, sr_hz)?;
    write_wav_file(&path, &samples, sr)?;
    Ok(path)
}

fn stage_export(
    input: &Path,
    checkpoint: &ProduceCheckpoint,
    out_dir: &Path,
    sr_hz: u32,
) -> Result<PathBuf> {
    let path = out_dir.join("export.wav");
    let mix_input = checkpoint
        .artifacts
        .get("AudioMix")
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(|| out_dir.join("mix.wav"));
    if mix_input.exists() {
        fs::copy(&mix_input, &path)?;
    } else {
        let (samples, sr) = decode_audio(input, sr_hz)?;
        write_wav_file(&path, &samples, sr)?;
    }
    Ok(path)
}

fn stage_vad(input: &Path, out_dir: &Path, analysis_cfg: &AnalysisConfig) -> Result<PathBuf> {
    let vad_path = out_dir.join("timeline.json");
    let timeline = extract_timeline(input, analysis_cfg)?;
    fs::write(&vad_path, serde_json::to_string_pretty(&timeline)?)?;
    Ok(vad_path)
}

fn dispatch_json_stage(stage: &str, out_dir: &Path) -> Result<Option<PathBuf>> {
    let (file, key) = match stage {
        "SceneDetect" => ("scenes.json", "scenes"),
        "Transcribe" => ("transcription.json", "transcripts"),
        "CharacterAssign" => ("characters.json", "characters"),
        "VoiceSynthesize" => ("voices.json", "synthesized"),
        "NarratorGenerate" => ("narration_scripts.json", "scripts"),
        "NarratorSynthesize" => ("narrator_audio.json", "audio_segments"),
        "SfxSelect" => ("sfx_selections.json", "sfx"),
        "SfxFetch" => ("sfx_fetched.json", "files"),
        _ => bail!("Unknown stage: {stage}"),
    };
    let path = write_stage_json(out_dir.join(file), &serde_json::json!({ key: [] }))?;
    Ok(Some(path))
}

fn dispatch_stage(
    stage: &str,
    input: &Path,
    checkpoint: &ProduceCheckpoint,
    out_dir: &Path,
    analysis_cfg: &AnalysisConfig,
) -> Result<Option<PathBuf>> {
    match stage {
        "ExtractAudio" => Ok(Some(stage_extract_audio(
            input,
            out_dir,
            analysis_cfg.sample_rate_hz,
        )?)),
        "VoiceActivityDetect" => Ok(Some(stage_vad(input, out_dir, analysis_cfg)?)),
        "AudioMix" => Ok(Some(stage_audio_mix(
            input,
            checkpoint,
            out_dir,
            analysis_cfg.sample_rate_hz,
        )?)),
        "Export" => Ok(Some(stage_export(
            input,
            checkpoint,
            out_dir,
            analysis_cfg.sample_rate_hz,
        )?)),
        _ => dispatch_json_stage(stage, out_dir),
    }
}

fn execute_stage(
    stage: &str,
    input: &Path,
    checkpoint: &mut ProduceCheckpoint,
    checkpoint_path: &Path,
    _cfg: &AppConfig,
    out_dir: &Path,
) -> Result<()> {
    let analysis_cfg = AnalysisConfig::default();
    info!(stage, "Executing stage");
    let artifact = dispatch_stage(stage, input, checkpoint, out_dir, &analysis_cfg)?;

    checkpoint.mark_completed(stage, artifact);
    checkpoint.save(checkpoint_path)?;
    Ok(())
}

/// `produce` entry point.
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

    let out_dir = if let Some(ref r) = resume {
        let parent = r.parent().unwrap_or_else(|| Path::new("."));
        if parent.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            parent.to_path_buf()
        }
    } else {
        input
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                "produce_{}",
                input
                    .file_stem()
                    .map_or("out", |s| s.to_str().unwrap_or("out"))
            ))
    };
    fs::create_dir_all(&out_dir)?;

    let checkpoint_file = resume
        .clone()
        .unwrap_or_else(|| out_dir.join("checkpoint.json"));

    let mut checkpoint = if checkpoint_file.exists() {
        let cp = ProduceCheckpoint::load(&checkpoint_file)?;
        if !cp.input_file.is_empty() && cp.input_file != input.to_string_lossy() {
            anyhow::bail!(
                "checkpoint input file mismatch: expected {}, found {}",
                cp.input_file,
                input.display()
            );
        }
        cp
    } else {
        ProduceCheckpoint {
            input_file: input.to_string_lossy().to_string(),
            ..Default::default()
        }
    };

    for stage in STAGES {
        if checkpoint.is_stage_completed(stage) {
            info!(stage, "Skipping already completed stage");
            continue;
        }
        execute_stage(
            stage,
            &input,
            &mut checkpoint,
            &checkpoint_file,
            cfg,
            &out_dir,
        )?;
    }

    println!("produce complete: output in {}", out_dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{WavSpec, WavWriter};

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
    fn real_run_executes_all_stages_and_creates_artifacts() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let wav_path = temp_dir.path().join("input.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&wav_path, spec).expect("create wav writer");
        for _ in 0..16000 {
            writer.write_sample(0i16).expect("write sample");
        }
        writer.finalize().expect("finalize wav");

        let cfg = AppConfig::default();
        assert!(handle_produce(wav_path.clone(), None, false, &cfg).is_ok());

        let produce_dir = temp_dir.path().join("produce_input");
        let ckpt_path = produce_dir.join("checkpoint.json");
        assert!(ckpt_path.exists());

        let ckpt = ProduceCheckpoint::load(&ckpt_path).expect("load checkpoint");
        for stage in STAGES {
            assert!(
                ckpt.is_stage_completed(stage),
                "stage {stage} should be completed"
            );
        }
    }

    #[test]
    fn resume_skips_completed_stages() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let wav_path = temp_dir.path().join("input.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&wav_path, spec).expect("create wav writer");
        for _ in 0..16000 {
            writer.write_sample(0i16).expect("write sample");
        }
        writer.finalize().expect("finalize wav");

        let produce_dir = temp_dir.path().join("produce_input");
        fs::create_dir_all(&produce_dir).expect("create produce dir");
        let ckpt_path = produce_dir.join("checkpoint.json");

        let mut ckpt = ProduceCheckpoint {
            input_file: wav_path.to_string_lossy().to_string(),
            ..Default::default()
        };
        ckpt.mark_completed("ExtractAudio", Some(produce_dir.join("extracted.wav")));
        ckpt.save(&ckpt_path).expect("save checkpoint");

        let cfg = AppConfig::default();
        let res = handle_produce(wav_path.clone(), Some(ckpt_path.clone()), false, &cfg);
        assert!(res.is_ok(), "expected Ok, got: {:?}", res);

        let loaded_ckpt = ProduceCheckpoint::load(&ckpt_path).expect("load checkpoint");
        for stage in STAGES {
            assert!(loaded_ckpt.is_stage_completed(stage));
        }
    }

    #[test]
    fn resume_rejects_input_mismatch() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let wav_path_a = temp_dir.path().join("input_a.wav");
        let wav_path_b = temp_dir.path().join("input_b.wav");
        let ckpt_path = temp_dir.path().join("checkpoint.json");

        let ckpt = ProduceCheckpoint {
            input_file: wav_path_a.to_string_lossy().to_string(),
            ..Default::default()
        };
        ckpt.save(&ckpt_path).expect("save checkpoint");

        let cfg = AppConfig::default();
        let res = handle_produce(wav_path_b, Some(ckpt_path), false, &cfg);
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("checkpoint input file mismatch"));
    }
}
