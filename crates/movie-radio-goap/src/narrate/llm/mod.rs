//! LLM-backed gap narration with a structured evidence contract (#412).
//!
//! The deterministic generator in the parent module stays the default. This
//! stage runs only when `narrator.llm.enabled` is set: it asks the configured
//! narrator backend for JSON that cites the evidence it used, then applies the
//! timing and filler rules of the acoustic path plus language, evidence and
//! unsupported-claim checks. Invalid output is never synthesized — after the
//! bounded retry budget the window is left unnarrated and recorded. Transport
//! failures fall back to the deterministic description with an explicit
//! reason so degradation is never silent.

use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use movie_radio_pipeline::narrator::NarratorAiBackend;
use movie_radio_types::{NarratorParams, RenderedPrompt, TimelineOutput, VisualGap};

mod contract;

pub use self::contract::NarrationRejection;
pub(crate) use self::contract::{
    parse_response, render_gap_prompt, request_from_context, validate_response,
};

use super::{NarrationEvidence, NarrationGenerator, NarrationScript};

/// Version of `templates/narrator_gap_prompt.md`; echoed into every request
/// and report so a stored narration can be traced to the prompt that made it.
pub const PROMPT_VERSION: &str = "gap-narration-v1";
pub(crate) const PROMPT_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../templates/narrator_gap_prompt.md"
));
const MAX_RETRY_ATTEMPTS: u32 = 3;
const MIN_GAP_CONFIDENCE: f32 = 0.4;
const MIN_GAP_DURATION_MS: u64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjacentContext {
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Everything the model is allowed to use, plus the constraints its answer
/// must satisfy. Serialized verbatim into the prompt as untrusted data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NarrationRequest {
    pub prompt_version: String,
    pub gap_start_ms: u64,
    pub gap_end_ms: u64,
    pub allowed_duration_ms: u64,
    pub words_per_minute: f64,
    pub max_words: usize,
    pub language: String,
    pub style: String,
    pub acoustic_evidence: Vec<EvidenceItem>,
    pub adjacent_context: AdjacentContext,
    pub visual_evidence: Vec<EvidenceItem>,
    pub forbidden_claims: Vec<String>,
    pub forbidden_filler: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pronunciation {
    pub term: String,
    pub hint: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrationResponse {
    pub text: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub uncertainty: Option<String>,
    #[serde(default)]
    pub pronunciation: Vec<Pronunciation>,
}

/// Provider abstraction so the retry/validation policy is testable without a
/// network. `ProviderBackend` adapts the shared pipeline dispatch.
#[async_trait::async_trait]
pub trait NarrationBackend: Send + Sync {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    async fn complete(&self, prompt: &str, params: &NarratorParams) -> Result<String>;
}

pub struct ProviderBackend {
    backend: Box<dyn NarratorAiBackend>,
    provider: String,
    model: String,
}

impl ProviderBackend {
    pub fn new(backend: Box<dyn NarratorAiBackend>, provider: &str, model: &str) -> Self {
        Self {
            backend,
            provider: provider.to_string(),
            model: model.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl NarrationBackend for ProviderBackend {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, prompt: &str, params: &NarratorParams) -> Result<String> {
        let rendered = RenderedPrompt {
            text: prompt.to_string(),
            variables: std::collections::HashMap::new(),
        };
        self.backend.generate_narration(&rendered, params).await
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LlmDecision {
    Accepted(NarrationResponse),
    Rejected {
        reason: NarrationRejection,
        attempts: u32,
    },
    ProviderUnavailable {
        error: String,
        attempts: u32,
    },
}

pub struct LlmNarrator {
    backend: Arc<dyn NarrationBackend>,
    max_attempts: u32,
}

impl LlmNarrator {
    pub fn new(backend: Arc<dyn NarrationBackend>, max_attempts: u32) -> Self {
        Self {
            backend,
            max_attempts: max_attempts.clamp(1, MAX_RETRY_ATTEMPTS),
        }
    }

    pub fn provider(&self) -> &str {
        self.backend.provider()
    }

    pub fn model(&self) -> &str {
        self.backend.model()
    }

    /// Runs at most `max_attempts` calls. Invalid answers are retried within
    /// that bound and then rejected; transport errors are likewise bounded and
    /// reported as unavailable so the caller can fall back.
    pub async fn narrate(
        &self,
        request: &NarrationRequest,
        params: &NarratorParams,
        previous_text: Option<&str>,
    ) -> LlmDecision {
        let prompt = render_gap_prompt(request);
        let mut attempts = 0;
        let mut last_rejection = None;
        let mut last_error = None;
        for _ in 0..self.max_attempts {
            attempts += 1;
            match self.backend.complete(&prompt, params).await {
                Ok(raw) => match parse_response(&raw).and_then(|response| {
                    validate_response(request, &response, previous_text).map(|()| response)
                }) {
                    Ok(response) => return LlmDecision::Accepted(response),
                    Err(reason) => {
                        tracing::warn!(
                            attempt = attempts,
                            reason = %reason,
                            "rejected LLM narration output"
                        );
                        last_rejection = Some(reason);
                        last_error = None;
                    }
                },
                Err(error) => {
                    tracing::warn!(
                        attempt = attempts,
                        error = %error,
                        "LLM narration provider error"
                    );
                    last_error = Some(error.to_string());
                }
            }
        }
        if let Some(error) = last_error {
            LlmDecision::ProviderUnavailable { error, attempts }
        } else if let Some(reason) = last_rejection {
            LlmDecision::Rejected { reason, attempts }
        } else {
            LlmDecision::ProviderUnavailable {
                error: "no attempts executed".to_string(),
                attempts,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmWindowOutcome {
    Accepted,
    Rejected,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmWindowReport {
    pub gap_start_ms: u64,
    pub gap_end_ms: u64,
    pub outcome: LlmWindowOutcome,
    pub reason: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmNarrationReport {
    pub prompt_version: String,
    pub provider: String,
    pub model: String,
    pub accepted: usize,
    pub rejected: usize,
    pub fallback: usize,
    pub windows: Vec<LlmWindowReport>,
}

pub struct LlmOutcome {
    pub scripts: Vec<NarrationScript>,
    pub report: LlmNarrationReport,
}

pub async fn generate_scripts(
    generator: &NarrationGenerator,
    narrator: &LlmNarrator,
    style: &str,
    params: &NarratorParams,
    timeline: &TimelineOutput,
    gaps: &[VisualGap],
) -> Result<LlmOutcome> {
    // Deterministic descriptions are the fallback for transport failures.
    let fallback = generator.generate(timeline, gaps)?;
    let mut scripts: Vec<NarrationScript> = Vec::new();
    let mut windows = Vec::new();
    let mut accepted = 0;
    let mut rejected = 0;
    let mut fallback_count = 0;

    let mut ordered: Vec<&VisualGap> = gaps.iter().collect();
    ordered.sort_by_key(|gap| gap.start_ms);
    for gap in ordered {
        if gap.confidence < MIN_GAP_CONFIDENCE {
            continue;
        }
        let gap_duration_ms = gap.end_ms.saturating_sub(gap.start_ms);
        if gap_duration_ms < MIN_GAP_DURATION_MS {
            continue;
        }
        let max_words = generator.max_words_for_duration(gap_duration_ms);
        if max_words == 0 {
            continue;
        }

        let context = generator.extract_context(timeline, gap);
        let request = request_from_context(
            &context,
            gap.start_ms,
            gap.end_ms,
            &generator.language,
            style,
            max_words,
            generator.words_per_minute,
        );
        let previous_text = scripts
            .last()
            .filter(|script| script.gap_end_ms == gap.start_ms)
            .map(|script| script.text.as_str());
        match narrator.narrate(&request, params, previous_text).await {
            LlmDecision::Accepted(response) => {
                let words = response.text.split_whitespace().count();
                let emotion = generator.infer_emotion(&context);
                scripts.push(NarrationScript {
                    gap_start_ms: gap.start_ms,
                    gap_end_ms: gap.end_ms,
                    text: response.text.trim().to_string(),
                    emotion,
                    word_count: words,
                    duration_ms: generator.estimate_duration_ms(words),
                    evidence: NarrationEvidence::Generated {
                        provider: narrator.provider().to_string(),
                        model: narrator.model().to_string(),
                        evidence_ids: response.evidence_ids.clone(),
                        uncertainty: response.uncertainty.clone(),
                        pronunciation: response.pronunciation.clone(),
                    },
                });
                accepted += 1;
                windows.push(LlmWindowReport {
                    gap_start_ms: gap.start_ms,
                    gap_end_ms: gap.end_ms,
                    outcome: LlmWindowOutcome::Accepted,
                    reason: None,
                    evidence_ids: response.evidence_ids,
                });
            }
            LlmDecision::Rejected { reason, attempts } => {
                // Never synthesize text that failed validation.
                rejected += 1;
                tracing::warn!(
                    gap_start_ms = gap.start_ms,
                    attempts,
                    reason = %reason,
                    "leaving gap unnarrated after rejected LLM output"
                );
                windows.push(LlmWindowReport {
                    gap_start_ms: gap.start_ms,
                    gap_end_ms: gap.end_ms,
                    outcome: LlmWindowOutcome::Rejected,
                    reason: Some(format!("{reason} (after {attempts} attempts)")),
                    evidence_ids: Vec::new(),
                });
            }
            LlmDecision::ProviderUnavailable { error, attempts } => {
                let mut script = fallback
                    .iter()
                    .find(|script| {
                        script.gap_start_ms == gap.start_ms && script.gap_end_ms == gap.end_ms
                    })
                    .cloned();
                if let Some(script) = script.as_mut() {
                    script.evidence = NarrationEvidence::AudioOnly {
                        reason: format!("llm provider unavailable: {error}"),
                    };
                    scripts.push(script.clone());
                }
                fallback_count += 1;
                let reason = format!("{error} (after {attempts} attempts)");
                tracing::warn!(
                    gap_start_ms = gap.start_ms,
                    attempts,
                    error = %error,
                    "LLM provider unavailable; using labelled acoustic fallback"
                );
                windows.push(LlmWindowReport {
                    gap_start_ms: gap.start_ms,
                    gap_end_ms: gap.end_ms,
                    outcome: LlmWindowOutcome::Fallback,
                    reason: Some(reason),
                    evidence_ids: Vec::new(),
                });
            }
        }
    }

    Ok(LlmOutcome {
        scripts,
        report: LlmNarrationReport {
            prompt_version: PROMPT_VERSION.to_string(),
            provider: narrator.provider().to_string(),
            model: narrator.model().to_string(),
            accepted,
            rejected,
            fallback: fallback_count,
            windows,
        },
    })
}
