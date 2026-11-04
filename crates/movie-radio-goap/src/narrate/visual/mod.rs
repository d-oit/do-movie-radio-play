//! Optional visual scene understanding for grounded audio description (#409).
//!
//! The stage samples representative frames from each candidate narration
//! window, asks a provider-agnostic vision model for structured observations,
//! rejects anything a frame cannot prove, and composes narration that fits the
//! existing speech-free window. Every window is recorded in a review report
//! with its evidence, and anything without visual evidence keeps the acoustic
//! description, explicitly labelled as such.

pub mod cache;
pub mod evidence;
pub mod frames;
pub mod grounding;
pub mod provider;
pub mod state;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use movie_radio_types::{TimelineOutput, VisualConfig, VisualGap};

use super::{NarrationEvidence, NarrationGenerator, NarrationScript};

pub use cache::{cache_key, FrameAnalysisCache};
/// Minimum gap confidence the acoustic generator narrates; the visual stage
/// uses the same bound so both paths cover identical windows.
const MIN_GAP_CONFIDENCE: f32 = 0.4;

pub use evidence::{
    EvidenceSource, FrameEvidence, NarrationWindow, VisualNarrationOutcome, VisualNarrationReport,
    WindowEvidence,
};
pub use frames::{
    sample_timestamps, FfmpegFrameExtractor, FrameExtractor, FrameSample, SamplingPolicy,
};
pub use grounding::{GroundedFrame, GroundingIssue, RejectedClaim};
pub use provider::{
    build_prompt, parse_analysis, FrameObservation, OllamaVision, OpenRouterVision, VisionAnalysis,
    VisionError, VisionErrorKind, VisionProvider, VisionRequest,
};
pub use state::VisualPipelineState;

pub struct VisualNarrator {
    config: VisualConfig,
    generator: NarrationGenerator,
    known_characters: Vec<String>,
    extractor: Arc<dyn FrameExtractor>,
    provider: Option<Arc<dyn VisionProvider>>,
    sampling: SamplingPolicy,
    cache: FrameAnalysisCache,
}

impl VisualNarrator {
    /// Builds the stage from configuration. `provider = "none"` leaves the
    /// audio-only path in place.
    pub fn new(config: VisualConfig, language: &str) -> Self {
        let provider: Option<Arc<dyn VisionProvider>> = match config.provider.as_str() {
            "openrouter" => match OpenRouterVision::from_config(&config) {
                Ok(provider) => Some(Arc::new(provider)),
                Err(err) => {
                    tracing::warn!(error = %err, "visual provider disabled by the LLM provider policy");
                    None
                }
            },
            "ollama" => Some(Arc::new(OllamaVision::from_config(&config))),
            _ => None,
        };
        let cache_dir = config
            .cache_dir
            .as_ref()
            .map_or_else(FrameAnalysisCache::default_dir, PathBuf::from);
        Self {
            sampling: SamplingPolicy::from_config(&config),
            config,
            generator: NarrationGenerator::default().with_language(language),
            known_characters: Vec::new(),
            extractor: Arc::new(FfmpegFrameExtractor::default()),
            provider,
            cache: FrameAnalysisCache::new(cache_dir),
        }
    }

    pub fn with_extractor(mut self, extractor: Arc<dyn FrameExtractor>) -> Self {
        self.extractor = extractor;
        self
    }

    pub fn with_provider(mut self, provider: Arc<dyn VisionProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn with_known_characters(mut self, names: Vec<String>) -> Self {
        self.known_characters = names;
        self
    }

    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        self.cache = FrameAnalysisCache::new(dir);
        self
    }

    pub async fn generate(
        &self,
        movie: &Path,
        timeline: &TimelineOutput,
        gaps: &[VisualGap],
    ) -> Result<VisualNarrationOutcome> {
        // Start from the acoustic narration: it already applies the gap
        // confidence, word-budget and duplicate rules, so visual evidence only
        // replaces text it can actually ground.
        let mut scripts = self.generator.generate(timeline, gaps)?;
        let mut windows = Vec::new();
        let mut remaining = self.sampling.max_total_frames;

        let mut ordered: Vec<&VisualGap> = gaps
            .iter()
            .filter(|gap| {
                gap.confidence >= MIN_GAP_CONFIDENCE
                    && NarrationWindow::from_gap(gap).is_narratable()
            })
            .collect();
        ordered.sort_by_key(|gap| gap.start_ms);

        for gap in ordered {
            let window = NarrationWindow::from_gap(gap);
            let max_words = self.generator.max_words_for_duration(window.duration_ms());
            let Some(script_index) = scripts
                .iter()
                .position(|s| s.gap_start_ms == gap.start_ms && s.gap_end_ms == gap.end_ms)
            else {
                windows.push(Self::audio_only(
                    window,
                    "window was left unnarrated by the acoustic planner",
                    Vec::new(),
                    Vec::new(),
                ));
                continue;
            };
            let provider = match self.provider.as_ref() {
                Some(provider) => Arc::clone(provider),
                None => {
                    let reason = "no visual provider configured";
                    Self::label_audio_only(&mut scripts[script_index], reason);
                    windows.push(Self::audio_only(window, reason, Vec::new(), Vec::new()));
                    continue;
                }
            };
            if remaining == 0 {
                let reason = "frame budget exhausted";
                Self::label_audio_only(&mut scripts[script_index], reason);
                windows.push(Self::audio_only(window, reason, Vec::new(), Vec::new()));
                continue;
            }
            let budget = remaining.min(self.sampling.max_frames_per_window);
            match self
                .analyse_window(movie, timeline, gap, window, max_words, budget, &provider)
                .await
            {
                Ok(analysis) => {
                    remaining = remaining.saturating_sub(analysis.frames_extracted as u32);
                    if let Some(text) = analysis.text {
                        let words = text.split_whitespace().count();
                        let script = &mut scripts[script_index];
                        script.text = text.clone();
                        script.word_count = words;
                        script.duration_ms = self.generator.estimate_duration_ms(words);
                        script.evidence = NarrationEvidence::Visual {
                            provider: provider.name().to_string(),
                            model: provider.model().to_string(),
                            frame_refs: analysis
                                .frames
                                .iter()
                                .map(|f| f.frame_ref.clone())
                                .collect(),
                            confidence: analysis.max_confidence,
                        };
                        windows.push(WindowEvidence {
                            gap_start_ms: window.start_ms,
                            gap_end_ms: window.end_ms,
                            source: EvidenceSource::Visual,
                            narration_text: text,
                            frames: analysis.frames,
                            rejected_claims: analysis.rejected,
                            fallback_reason: None,
                        });
                    } else {
                        let reason = analysis
                            .fallback_reason
                            .clone()
                            .unwrap_or_else(|| "no grounded visual evidence".to_string());
                        Self::label_audio_only(&mut scripts[script_index], &reason);
                        windows.push(WindowEvidence {
                            gap_start_ms: window.start_ms,
                            gap_end_ms: window.end_ms,
                            source: EvidenceSource::AudioOnly,
                            narration_text: scripts[script_index].text.clone(),
                            frames: analysis.frames,
                            rejected_claims: analysis.rejected,
                            fallback_reason: Some(reason),
                        });
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        gap_start_ms = window.start_ms,
                        error = %error,
                        "visual narration failed for window; using acoustic description"
                    );
                    let reason = format!("vision stage failed: {error}");
                    Self::label_audio_only(&mut scripts[script_index], &reason);
                    windows.push(Self::audio_only(window, &reason, Vec::new(), Vec::new()));
                }
            }
        }

        let visual_windows = windows
            .iter()
            .filter(|w| w.source == EvidenceSource::Visual)
            .count();
        let report = VisualNarrationReport {
            movie: movie.to_string_lossy().to_string(),
            language: self.generator.language.clone(),
            provider: self
                .provider
                .as_ref()
                .map_or_else(|| "none".to_string(), |p| p.name().to_string()),
            model: self
                .provider
                .as_ref()
                .map_or_else(|| self.config.model.clone(), |p| p.model().to_string()),
            sampling: self.sampling.clone(),
            visual_windows,
            audio_only_windows: windows.len().saturating_sub(visual_windows),
            windows,
        };
        Ok(VisualNarrationOutcome { scripts, report })
    }

    #[allow(clippy::too_many_arguments)]
    async fn analyse_window(
        &self,
        movie: &Path,
        timeline: &TimelineOutput,
        gap: &VisualGap,
        window: NarrationWindow,
        max_words: usize,
        budget: u32,
        provider: &Arc<dyn VisionProvider>,
    ) -> Result<WindowAnalysis> {
        let provider_name = provider.name();
        let provider_model = provider.model();
        let key = cache_key(
            movie,
            window,
            &self.sampling,
            provider_name,
            provider_model,
            &self.generator.language,
        );
        let (frames, analysis) = match self.cache.load(&key) {
            Ok(Some(hit)) => hit,
            _ => {
                let out_dir = self.cache.frame_dir(&key);
                let frames = self
                    .extractor
                    .extract_window(movie, window, &self.sampling, budget, &out_dir)
                    .await
                    .context("extract representative frames")?;
                if frames.is_empty() {
                    return Ok(WindowAnalysis {
                        frames: Vec::new(),
                        rejected: Vec::new(),
                        text: None,
                        fallback_reason: Some(
                            "no representative frames could be extracted".to_string(),
                        ),
                        frames_extracted: 0,
                        max_confidence: 0.0,
                    });
                }
                let (before, after) = neighbour_context(timeline, gap);
                let request = VisionRequest {
                    language: &self.generator.language,
                    window_start_ms: window.start_ms,
                    window_end_ms: window.end_ms,
                    max_words,
                    acoustic_tags: &gap.tags,
                    before_context: &before,
                    after_context: &after,
                    known_characters: &self.known_characters,
                    frames: &frames,
                };
                let timeout = Duration::from_secs(self.config.timeout_secs.max(1));
                let raw = tokio::time::timeout(timeout, provider.describe(&request))
                    .await
                    .map_err(|_| {
                        anyhow!(
                            "vision provider {} timed out after {}s",
                            provider_name,
                            timeout.as_secs()
                        )
                    })??;
                let analysis = parse_analysis(&raw, &frames)?;
                if let Err(error) = self.cache.store(&key, &frames, &analysis) {
                    tracing::warn!(error = %error, "could not cache vision analysis");
                }
                (frames, analysis)
            }
        };

        let policy = grounding::GroundingPolicy::new(
            self.known_characters.clone(),
            self.config.min_confidence,
        );
        let mut grounded = grounding::ground(&analysis, &policy);
        grounded.sort_by_key(|frame| frame.timestamp_ms);

        let mut sentences = Vec::new();
        for frame in &grounded {
            if !frame.usable {
                continue;
            }
            for sentence in &frame.sentences {
                if !sentences
                    .iter()
                    .any(|existing: &String| existing == sentence)
                {
                    sentences.push(sentence.clone());
                }
            }
        }
        let rejected = grounded
            .iter()
            .flat_map(|frame| frame.rejected.iter().cloned())
            .collect::<Vec<_>>();
        let frames_evidence = grounded
            .iter()
            .map(|frame| {
                let path = frames
                    .iter()
                    .find(|sample| sample.frame_ref == frame.frame_ref)
                    .map_or_else(String::new, |sample| sample.path.display().to_string());
                FrameEvidence {
                    frame_ref: frame.frame_ref.clone(),
                    timestamp_ms: frame.timestamp_ms,
                    confidence: frame.confidence,
                    description: frame.sentences.join(" "),
                    uncertainty: frame.uncertainty.clone(),
                    path,
                }
            })
            .collect::<Vec<_>>();
        let max_confidence = grounded
            .iter()
            .map(|frame| frame.confidence)
            .fold(0.0_f32, f32::max);
        let text = self.generator.compose_sentences(&sentences, max_words);
        let fallback_reason = if text.is_empty() {
            Some(if sentences.is_empty() {
                "no frame passed grounding and confidence checks".to_string()
            } else {
                "grounded description did not fit the window's word budget".to_string()
            })
        } else {
            None
        };
        Ok(WindowAnalysis {
            frames: frames_evidence,
            rejected,
            text: (!text.is_empty()).then_some(text),
            fallback_reason,
            frames_extracted: frames.len(),
            max_confidence,
        })
    }

    fn audio_only(
        window: NarrationWindow,
        reason: &str,
        frames: Vec<FrameEvidence>,
        rejected: Vec<RejectedClaim>,
    ) -> WindowEvidence {
        WindowEvidence {
            gap_start_ms: window.start_ms,
            gap_end_ms: window.end_ms,
            source: EvidenceSource::AudioOnly,
            narration_text: String::new(),
            frames,
            rejected_claims: rejected,
            fallback_reason: Some(reason.to_string()),
        }
    }

    fn label_audio_only(script: &mut NarrationScript, reason: &str) {
        script.evidence = NarrationEvidence::AudioOnly {
            reason: reason.to_string(),
        };
    }
}

struct WindowAnalysis {
    frames: Vec<FrameEvidence>,
    rejected: Vec<RejectedClaim>,
    text: Option<String>,
    fallback_reason: Option<String>,
    frames_extracted: usize,
    max_confidence: f32,
}

fn neighbour_context(timeline: &TimelineOutput, gap: &VisualGap) -> (String, String) {
    let describe = |segment: &movie_radio_types::Segment| {
        if segment.tags.is_empty() {
            format!("{:?}", segment.kind)
        } else {
            format!("{:?}, tags: {}", segment.kind, segment.tags.join(", "))
        }
    };
    let before = timeline
        .segments
        .iter()
        .rfind(|segment| segment.end_ms <= gap.start_ms)
        .map_or_else(|| "none".to_string(), describe);
    let after = timeline
        .segments
        .iter()
        .find(|segment| segment.start_ms >= gap.end_ms)
        .map_or_else(|| "none".to_string(), describe);
    (before, after)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_grounding;
