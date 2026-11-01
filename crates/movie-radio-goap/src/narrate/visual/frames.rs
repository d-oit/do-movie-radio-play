//! Bounded representative-frame extraction for one narration window.
//!
//! Only the gap window is decoded (never the whole movie), the number of
//! extracted frames is capped by the configured frame budget, and frames are
//! chosen at scene changes when the window has any, else evenly spread.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

use movie_radio_types::VisualConfig;

use super::NarrationWindow;

/// Hard cap mirrors `narrator.visual.max_frames_per_window` validation.
pub const MAX_FRAMES_PER_WINDOW: u32 = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SamplingPolicy {
    pub max_frames_per_window: u32,
    pub max_total_frames: u32,
    pub scene_change_threshold: f32,
    pub frame_width: u32,
}

impl SamplingPolicy {
    pub fn from_config(cfg: &VisualConfig) -> Self {
        Self {
            max_frames_per_window: cfg.max_frames_per_window.clamp(1, MAX_FRAMES_PER_WINDOW),
            max_total_frames: cfg.max_total_frames.max(1),
            scene_change_threshold: cfg.scene_change_threshold.clamp(0.0, 1.0),
            frame_width: cfg.frame_width.clamp(64, 1920),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSample {
    pub frame_ref: String,
    pub timestamp_ms: u64,
    pub path: PathBuf,
    /// JPEG bytes used for the vision request; not serialized.
    pub bytes: Vec<u8>,
}

/// Extracts representative frames for one speech-free window.
#[async_trait::async_trait]
pub trait FrameExtractor: Send + Sync {
    async fn extract_window(
        &self,
        movie: &Path,
        window: NarrationWindow,
        policy: &SamplingPolicy,
        budget: u32,
        out_dir: &Path,
    ) -> Result<Vec<FrameSample>>;
}

/// Picks at most `limit` timestamps inside `window`, preferring detected scene
/// changes and filling the remaining budget evenly. Pure, so the sampling
/// policy is testable without touching a video file.
pub fn sample_timestamps(window: NarrationWindow, scene_times: &[u64], limit: u32) -> Vec<u64> {
    let limit = limit.min(MAX_FRAMES_PER_WINDOW) as usize;
    if limit == 0 || window.end_ms <= window.start_ms {
        return Vec::new();
    }
    let mut times: Vec<u64> = scene_times
        .iter()
        .copied()
        .filter(|t| *t >= window.start_ms && *t < window.end_ms)
        .collect();
    times.sort_unstable();
    times.dedup();
    if times.len() > limit {
        // Keep scene changes spread across the window instead of only the first.
        let step = times.len() as f64 / limit as f64;
        times = (0..limit)
            .map(|i| times[(i as f64 * step) as usize])
            .collect();
        times.dedup();
    }
    let span = window.end_ms - window.start_ms;
    while times.len() < limit {
        let need = limit - times.len();
        let mut added = false;
        for i in 0..need {
            let t = window.start_ms + span * (i as u64 + 1) / (need as u64 + 1);
            if !times.iter().any(|existing| existing.abs_diff(t) < 100) {
                times.push(t);
                added = true;
            }
        }
        if !added {
            break;
        }
        times.sort_unstable();
        times.dedup();
    }
    times.truncate(limit);
    times
}

#[derive(Debug, Clone)]
pub struct FfmpegFrameExtractor {
    pub binary: String,
}

impl Default for FfmpegFrameExtractor {
    fn default() -> Self {
        Self {
            binary: "ffmpeg".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl FrameExtractor for FfmpegFrameExtractor {
    async fn extract_window(
        &self,
        movie: &Path,
        window: NarrationWindow,
        policy: &SamplingPolicy,
        budget: u32,
        out_dir: &Path,
    ) -> Result<Vec<FrameSample>> {
        let binary = self.binary.clone();
        let movie = movie.to_path_buf();
        let policy = policy.clone();
        let out_dir = out_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            extract_blocking(&binary, &movie, window, &policy, budget, &out_dir)
        })
        .await
        .context("frame extraction task failed")?
    }
}

fn extract_blocking(
    binary: &str,
    movie: &Path,
    window: NarrationWindow,
    policy: &SamplingPolicy,
    budget: u32,
    out_dir: &Path,
) -> Result<Vec<FrameSample>> {
    if budget == 0 {
        return Ok(Vec::new());
    }
    let scene_times = scene_timestamps(binary, movie, window, policy.scene_change_threshold)?;
    let times = sample_timestamps(window, &scene_times, budget);
    if times.is_empty() {
        return Ok(Vec::new());
    }

    let _ = std::fs::remove_dir_all(out_dir);
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("cannot create frame cache dir {}", out_dir.display()))?;

    let start = window.start_ms as f64 / 1000.0;
    let end = window.end_ms as f64 / 1000.0;
    let expr = times
        .iter()
        .map(|ts| {
            let t = (*ts as f64 / 1000.0 - start).max(0.0);
            format!("between(t,{:.3},{:.3})", (t - 0.06).max(0.0), t + 0.06)
        })
        .collect::<Vec<_>>()
        .join("+");
    let filter = format!("select='{expr}',scale={}:-2", policy.frame_width);
    let pattern = out_dir.join("frame-%03d.jpg");
    let frames_cap = budget.to_string();
    let output = Command::new(binary)
        .args([
            "-hide_banner",
            "-nostats",
            "-loglevel",
            "error",
            "-ss",
            &format!("{start:.3}"),
            "-to",
            &format!("{end:.3}"),
            "-i",
        ])
        .arg(movie)
        .args(["-an", "-vf", &filter, "-frames:v", &frames_cap, "-q:v", "3"])
        .arg(&pattern)
        .output()
        .with_context(|| format!("cannot run {binary}"))?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr);
        bail!(
            "ffmpeg frame extraction failed: {}",
            tail.lines().last().unwrap_or("").trim()
        );
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(out_dir)
        .with_context(|| format!("cannot read {}", out_dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("jpg"))
        .collect();
    files.sort();

    // A select expression can skip a planned timestamp on variable-frame-rate
    // input. Fall back to one bounded decode per timestamp rather than
    // mismatching frame files with timestamps.
    if files.len() != times.len() {
        return extract_each(binary, movie, &times, policy, out_dir);
    }

    let mut samples = Vec::with_capacity(files.len());
    for (i, path) in files.into_iter().enumerate() {
        if let Some(ts) = times.get(i) {
            samples.push(read_frame(&path, *ts, i)?);
        }
    }
    Ok(samples)
}

fn extract_each(
    binary: &str,
    movie: &Path,
    times: &[u64],
    policy: &SamplingPolicy,
    out_dir: &Path,
) -> Result<Vec<FrameSample>> {
    let _ = std::fs::remove_dir_all(out_dir);
    std::fs::create_dir_all(out_dir)?;
    let mut samples = Vec::new();
    for (i, ts) in times.iter().enumerate() {
        let path = out_dir.join(format!("frame-{i:03}.jpg"));
        let seconds = *ts as f64 / 1000.0;
        let width = policy.frame_width.to_string();
        let output = Command::new(binary)
            .args([
                "-hide_banner",
                "-nostats",
                "-loglevel",
                "error",
                "-ss",
                &format!("{seconds:.3}"),
                "-i",
            ])
            .arg(movie)
            .args([
                "-an",
                "-frames:v",
                "1",
                "-vf",
                &format!("scale={width}:-2"),
                "-q:v",
                "3",
            ])
            .arg(&path)
            .output()
            .with_context(|| format!("cannot run {binary}"))?;
        if output.status.success() && path.exists() {
            samples.push(read_frame(&path, *ts, i)?);
        }
    }
    Ok(samples)
}

fn read_frame(path: &Path, timestamp_ms: u64, index: usize) -> Result<FrameSample> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(FrameSample {
        frame_ref: format!("frame-{index:02}@{timestamp_ms}ms"),
        timestamp_ms,
        path: path.to_path_buf(),
        bytes,
    })
}

/// Scene-change timestamps inside the window, in absolute milliseconds.
/// `ffmpeg` resets timestamps after `-ss`, so the window offset is added back.
fn scene_timestamps(
    binary: &str,
    movie: &Path,
    window: NarrationWindow,
    threshold: f32,
) -> Result<Vec<u64>> {
    let start = window.start_ms as f64 / 1000.0;
    let end = window.end_ms as f64 / 1000.0;
    let filter = format!("select='gt(scene,{threshold:.3})',showinfo");
    let output = Command::new(binary)
        .args([
            "-hide_banner",
            "-nostats",
            "-loglevel",
            "info",
            "-ss",
            &format!("{start:.3}"),
            "-to",
            &format!("{end:.3}"),
            "-i",
        ])
        .arg(movie)
        .args(["-an", "-vf", &filter, "-f", "null", "-"])
        .output()
        .with_context(|| format!("cannot run {binary}"))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(stderr
        .lines()
        .filter_map(|line| line.split("pts_time:").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter_map(|secs| secs.parse::<f64>().ok())
        .map(|secs| window.start_ms + (secs * 1000.0).max(0.0) as u64)
        .filter(|ts| *ts >= window.start_ms && *ts < window.end_ms)
        .collect())
}
