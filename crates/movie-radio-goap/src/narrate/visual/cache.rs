//! On-disk cache for extracted frames and provider analysis.
//!
//! The key covers the input fingerprint (movie identity), window, sampling
//! policy, language and provider/model version, so a changed model or policy
//! never reuses stale descriptions.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::frames::{FrameSample, SamplingPolicy};
use super::provider::VisionAnalysis;
use super::NarrationWindow;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedFrame {
    frame_ref: String,
    timestamp_ms: u64,
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedWindow {
    frames: Vec<CachedFrame>,
    analysis: VisionAnalysis,
}

#[derive(Debug, Clone)]
pub struct FrameAnalysisCache {
    dir: PathBuf,
}

impl FrameAnalysisCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// User cache directory, honouring `XDG_CACHE_HOME` and `HOME`; without
    /// either it falls back to a repository-relative `.cache` directory
    /// rather than a shared temporary directory.
    pub fn default_dir() -> PathBuf {
        if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
            return PathBuf::from(dir).join("do-movie-radio-play/vision");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".cache/do-movie-radio-play/vision");
        }
        PathBuf::from(".cache/do-movie-radio-play/vision")
    }

    pub fn frame_dir(&self, key: &str) -> PathBuf {
        self.dir.join(key)
    }

    pub fn load(&self, key: &str) -> Result<Option<(Vec<FrameSample>, VisionAnalysis)>> {
        let path = self.entry_path(key);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        let cached: CachedWindow = match serde_json::from_str(&raw) {
            Ok(cached) => cached,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "ignoring corrupt vision cache entry");
                return Ok(None);
            }
        };
        let mut frames = Vec::with_capacity(cached.frames.len());
        for frame in &cached.frames {
            let bytes = match std::fs::read(&frame.path) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(None),
            };
            frames.push(FrameSample {
                frame_ref: frame.frame_ref.clone(),
                timestamp_ms: frame.timestamp_ms,
                path: frame.path.clone(),
                bytes,
            });
        }
        if frames.is_empty() {
            return Ok(None);
        }
        Ok(Some((frames, cached.analysis)))
    }

    pub fn store(
        &self,
        key: &str,
        frames: &[FrameSample],
        analysis: &VisionAnalysis,
    ) -> Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("cannot create vision cache {}", self.dir.display()))?;
        let cached = CachedWindow {
            frames: frames
                .iter()
                .map(|frame| CachedFrame {
                    frame_ref: frame.frame_ref.clone(),
                    timestamp_ms: frame.timestamp_ms,
                    path: frame.path.clone(),
                })
                .collect(),
            analysis: analysis.clone(),
        };
        let path = self.entry_path(key);
        let temp = path.with_extension("json.tmp");
        let json = serde_json::to_string(&cached).context("serialize vision cache entry")?;
        std::fs::write(&temp, json).with_context(|| format!("cannot write {}", temp.display()))?;
        std::fs::rename(&temp, &path)
            .with_context(|| format!("cannot commit {}", path.display()))?;
        Ok(())
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }
}

/// Stable cache key over movie identity, window, policy and provider version.
pub fn cache_key(
    movie: &Path,
    window: NarrationWindow,
    policy: &SamplingPolicy,
    provider: &str,
    model: &str,
    language: &str,
) -> String {
    let identity = std::fs::metadata(movie).map_or_else(
        |_| format!("{}:missing", movie.display()),
        |meta| {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{}:{}:{modified}", movie.display(), meta.len())
        },
    );
    crate::production::fingerprint(&[
        ("movie", identity),
        ("window", format!("{}-{}", window.start_ms, window.end_ms)),
        (
            "policy",
            format!(
                "{}-{}-{:.3}-{}",
                policy.max_frames_per_window,
                policy.max_total_frames,
                policy.scene_change_threshold,
                policy.frame_width
            ),
        ),
        ("provider", provider.to_string()),
        ("model", model.to_string()),
        ("language", language.to_string()),
    ])
}
