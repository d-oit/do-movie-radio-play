//! Commits and restores the artifacts a production stage produces, so a
//! checkpointed stage can be reused without re-running its work.
//!
//! Every artifact is written to a sibling temp path and renamed into place:
//! an interrupted stage never leaves a truncated artifact that a later resume
//! would accept.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::narrate::NarrationScript;
use crate::PipelineContext;
use movie_radio_pipeline::pipeline::decode::decode_audio;
use movie_radio_types::GapAnalysisOutput;
use movie_radio_verification::VerificationReport;

use super::checkpoint::part_sibling;
use super::ProductionStage;

const DECODED_AUDIO: &str = "decoded.wav";
const TIMELINE_JSON: &str = "timeline.json";
const GAPS_JSON: &str = "gaps.json";
const SCRIPTS_JSON: &str = "scripts.json";
const VISUAL_REVIEW_JSON: &str = "visual-review.json";
const NARRATION_INDEX_JSON: &str = "narration.json";
const VERIFICATION_JSON: &str = "verification.json";
const MASTERING_JSON: &str = "mastering-report.json";
const ASSEMBLED_MIX_WAV: &str = "assembled-mix.wav";
const TRACK_MAP_JSON: &str = "track-map.json";
const SFX_ASSETS_JSON: &str = "sfx-assets.json";

/// Persists the stage's artifacts and returns their keys mapped to paths.
pub fn persist_stage_artifacts(
    ctx: &PipelineContext,
    stage: &ProductionStage,
    dir: &Path,
) -> Result<BTreeMap<String, String>> {
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let mut artifacts = BTreeMap::new();
    match stage.name {
        "decode_audio" => {
            let samples = ctx
                .original_audio
                .as_ref()
                .context("cannot commit decoded audio: no samples in context")?;
            let path = dir.join(DECODED_AUDIO);
            commit_wav(&path, samples, ctx.sample_rate)?;
            artifacts.insert("decoded_audio".to_string(), path_string(&path));
        }
        "analyze_speech" => {
            let timeline = ctx.timeline.as_ref().context("no timeline to commit")?;
            let path = dir.join(TIMELINE_JSON);
            commit_json(&path, timeline)?;
            artifacts.insert("timeline".to_string(), path_string(&path));
        }
        "identify_gaps" => {
            let gaps: &GapAnalysisOutput = ctx
                .gap_analysis
                .as_ref()
                .context("no gap analysis to commit")?;
            let path = dir.join(GAPS_JSON);
            commit_json(&path, gaps)?;
            artifacts.insert("gaps".to_string(), path_string(&path));
        }
        "plan_narration" => {
            let scripts: &Vec<NarrationScript> = ctx
                .scripts
                .as_ref()
                .context("no narration scripts to commit")?;
            let path = dir.join(SCRIPTS_JSON);
            commit_json(&path, scripts)?;
            artifacts.insert("scripts".to_string(), path_string(&path));
            if let Some(report) = ctx.visual.report.as_ref() {
                let report_path = dir.join(VISUAL_REVIEW_JSON);
                commit_json(&report_path, report)?;
                artifacts.insert("visual_review".to_string(), path_string(&report_path));
            }
        }
        "synthesize_narration" => {
            persist_narration(ctx, dir, &mut artifacts)?;
        }
        "assemble_radio_play" => {
            let deliverable = &ctx.output_path;
            if !deliverable.exists() {
                bail!("assembled deliverable missing: {}", deliverable.display());
            }
            artifacts.insert("deliverable".to_string(), path_string(deliverable));
            // Keep the pre-mastering mix so a resumed run compares preservation
            // against the same signal, not the gain-adjusted deliverable.
            let path = dir.join(ASSEMBLED_MIX_WAV);
            if let Some(mix) = ctx.assembled_mix.as_ref() {
                commit_raw_mix_wav(&path, mix.path(), ctx.sample_rate)?;
            } else {
                let mix = ctx
                    .assembled_audio
                    .as_ref()
                    .context("cannot commit assembled mix: no samples in context")?;
                commit_wav(&path, mix, ctx.sample_rate)?;
            }
            artifacts.insert("assembled_mix".to_string(), path_string(&path));
            // Source/output intervals for narration and SFX, used by the run
            // manifest to link every item to where it landed.
            if let Some(track_map) = ctx.track_map.as_ref() {
                let map_path = dir.join(TRACK_MAP_JSON);
                commit_json(&map_path, track_map)?;
                artifacts.insert("track_map".to_string(), path_string(&map_path));
            }
            if !ctx.sfx_assets.is_empty() {
                let assets_path = dir.join(SFX_ASSETS_JSON);
                commit_json(&assets_path, &ctx.sfx_assets)?;
                artifacts.insert("sfx_assets".to_string(), path_string(&assets_path));
            }
        }
        "verify_quality" => {
            let report: &VerificationReport = ctx
                .verification
                .as_ref()
                .context("no verification report to commit")?;
            let path = dir.join(VERIFICATION_JSON);
            commit_json(&path, report)?;
            artifacts.insert("verification".to_string(), path_string(&path));
            if let Some(mastering) = ctx.mastering_report.as_ref() {
                let path = dir.join(MASTERING_JSON);
                commit_json(&path, mastering)?;
                artifacts.insert("mastering_report".to_string(), path_string(&path));
            }
        }
        "apply_learnings" => {
            let path = ctx
                .learning_state_path
                .as_ref()
                .filter(|p| p.exists())
                .context("learning state was not persisted; set a writable learning state path")?;
            artifacts.insert("learning_state".to_string(), path_string(path));
        }
        other => bail!("no artifact persistence for production stage '{other}'"),
    }
    Ok(artifacts)
}

/// Restores a checkpointed stage's artifacts into the pipeline context.
pub fn restore_stage_artifacts(
    ctx: &mut PipelineContext,
    stage: &ProductionStage,
    artifacts: &BTreeMap<String, String>,
) -> Result<()> {
    match stage.name {
        "decode_audio" => {
            let path = artifact_path(artifacts, "decoded_audio")?;
            let (samples, sample_rate) = read_wav(path)?;
            ctx.sample_rate = sample_rate;
            ctx.original_audio = Some(samples);
        }
        "analyze_speech" => {
            ctx.timeline = Some(read_json(artifact_path(artifacts, "timeline")?)?);
        }
        "identify_gaps" => {
            ctx.gap_analysis = Some(read_json(artifact_path(artifacts, "gaps")?)?);
        }
        "plan_narration" => {
            ctx.scripts = Some(read_json(artifact_path(artifacts, "scripts")?)?);
            if let Some(path) = artifacts.get("visual_review") {
                ctx.visual.report = serde_json::from_str(&std::fs::read_to_string(path)?).ok();
            }
        }
        "synthesize_narration" => {
            restore_narration(ctx, artifacts)?;
        }
        "assemble_radio_play" => {
            match artifacts.get("assembled_mix") {
                Some(path) => {
                    let (samples, sample_rate) = read_wav(Path::new(path))?;
                    ctx.sample_rate = sample_rate;
                    ctx.assembled_audio = Some(samples);
                }
                None => {
                    let path = artifact_path(artifacts, "deliverable")?;
                    let (samples, _) = decode_audio(path, ctx.sample_rate)?;
                    ctx.assembled_audio = Some(samples);
                }
            }
            if let Some(path) = artifacts.get("sfx_assets") {
                ctx.sfx_assets = read_json(Path::new(path)).context("cannot restore SFX assets")?;
            }
        }
        "verify_quality" => {
            ctx.verification = Some(read_json(artifact_path(artifacts, "verification")?)?);
            if let Some(path) = artifacts.get("mastering_report") {
                ctx.mastering_report =
                    Some(read_json(Path::new(path)).context("cannot restore mastering report")?);
            }
        }
        "apply_learnings" => {
            // The action already wrote the learning state to the recorded path.
        }
        other => bail!("no artifact restore for production stage '{other}'"),
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct NarrationIndex {
    entries: Vec<NarrationEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NarrationEntry {
    index: usize,
    file: Option<String>,
    provider: Option<String>,
    sample_rate_hz: u32,
}

fn persist_narration(
    ctx: &PipelineContext,
    dir: &Path,
    artifacts: &mut BTreeMap<String, String>,
) -> Result<()> {
    let mut entries = Vec::with_capacity(ctx.narration_audio.len());
    for (i, audio) in ctx.narration_audio.iter().enumerate() {
        let provider = ctx.narration_provider.get(i).cloned().flatten();
        let file = match audio {
            Some(audio) => {
                let path = dir.join(format!("narration_{i:04}.wav"));
                commit_wav(&path, &audio.samples, audio.sample_rate_hz)?;
                artifacts.insert(format!("narration_{i:04}"), path_string(&path));
                Some(path_string(&path))
            }
            None => None,
        };
        entries.push(NarrationEntry {
            index: i,
            file,
            provider,
            sample_rate_hz: audio.as_ref().map_or(ctx.sample_rate, |a| a.sample_rate_hz),
        });
    }
    let index_path = dir.join(NARRATION_INDEX_JSON);
    commit_json(&index_path, &NarrationIndex { entries })?;
    artifacts.insert("narration_index".to_string(), path_string(&index_path));
    Ok(())
}

fn restore_narration(
    ctx: &mut PipelineContext,
    artifacts: &BTreeMap<String, String>,
) -> Result<()> {
    let index: NarrationIndex = read_json(artifact_path(artifacts, "narration_index")?)?;
    ctx.narration_audio.clear();
    ctx.narration_provider.clear();
    for entry in index.entries {
        match entry.file {
            Some(file) => {
                let (samples, sample_rate_hz) = read_wav(Path::new(&file))?;
                ctx.narration_audio
                    .push(Some(movie_radio_voice::AudioOutput {
                        samples,
                        sample_rate_hz,
                    }));
            }
            None => ctx.narration_audio.push(None),
        }
        ctx.narration_provider.push(entry.provider);
    }
    Ok(())
}

fn artifact_path<'a>(artifacts: &'a BTreeMap<String, String>, key: &str) -> Result<&'a Path> {
    artifacts
        .get(key)
        .map(Path::new)
        .with_context(|| format!("checkpoint is missing artifact '{key}'"))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn commit_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = part_sibling(path);
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(&tmp, bytes).with_context(|| format!("cannot write {}", tmp.display()))?;
    commit(&tmp, path)
}

/// Commits a raw little-endian `f32` mono mix as a 16-bit WAV artifact,
/// streaming so the mix is never loaded whole.
fn commit_raw_mix_wav(path: &Path, raw_path: &Path, sample_rate_hz: u32) -> Result<()> {
    use std::io::Read;

    let tmp = part_sibling(path);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    {
        let mut writer = hound::WavWriter::create(&tmp, spec)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        let mut reader = std::io::BufReader::new(
            fs::File::open(raw_path)
                .with_context(|| format!("cannot open {}", raw_path.display()))?,
        );
        let mut bytes = vec![0u8; 1 << 22];
        loop {
            let read = reader.read(&mut bytes)?;
            if read == 0 {
                break;
            }
            for chunk in bytes[..read].as_chunks::<4>().0 {
                let sample = f32::from_le_bytes(*chunk);
                writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
            }
        }
        writer.finalize()?;
    }
    commit(&tmp, path)
}

fn commit_wav(path: &Path, samples: &[f32], sample_rate_hz: u32) -> Result<()> {
    let tmp = part_sibling(path);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    {
        let mut writer = hound::WavWriter::create(&tmp, spec)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        for &sample in samples {
            writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        writer.finalize()?;
    }
    commit(&tmp, path)
}

fn commit(tmp: &Path, path: &Path) -> Result<()> {
    fs::rename(tmp, path).with_context(|| {
        format!(
            "cannot commit artifact {} -> {}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("cannot parse {}", path.display()))
}

fn read_wav(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<std::result::Result<_, _>>()?
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<_, _>>()?,
    };
    Ok((samples, spec.sample_rate))
}
