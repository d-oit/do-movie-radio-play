//! Prompt contract, structured parsing and deterministic validation (#412).
//!
//! Everything here is pure: no network, no clock, no randomness, so the same
//! model answer always yields the same accept/reject decision.

use std::fmt;

use super::{AdjacentContext, EvidenceItem, NarrationRequest, NarrationResponse, PROMPT_TEMPLATE};

const MAX_FIELD_CHARS: usize = 600;
const VISUAL_CLAIM_MARKERS: [&str; 8] = [
    "we see",
    "the camera",
    "on screen",
    "the screen shows",
    "wir sehen",
    "man sieht",
    "die kamera",
    "im bild",
];
const GERMAN_MARKERS: [&str; 16] = [
    "der", "die", "das", "und", "ist", "ein", "eine", "mit", "von", "im", "zu", "sich", "auf",
    "nicht", "dem", "den",
];
const ENGLISH_MARKERS: [&str; 16] = [
    "the", "and", "is", "a", "an", "of", "to", "in", "it", "not", "this", "that", "on", "at",
    "with", "for",
];

/// Why a model answer was not usable. Each variant maps to a user-visible
/// reason in the narration report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NarrationRejection {
    InvalidJson(String),
    Empty,
    TooManyWords { words: usize, max: usize },
    OverBudget { estimated_ms: u64, allowed_ms: u64 },
    WrongLanguage { expected: String, detected: String },
    BannedFiller(String),
    MissingEvidence,
    UnknownEvidence(String),
    UnsupportedClaim(String),
    Repeated,
}

impl fmt::Display for NarrationRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(f, "invalid JSON: {error}"),
            Self::Empty => write!(f, "empty narration text"),
            Self::TooManyWords { words, max } => {
                write!(f, "narration has {words} words, budget is {max}")
            }
            Self::OverBudget {
                estimated_ms,
                allowed_ms,
            } => write!(
                f,
                "estimated speech {estimated_ms} ms exceeds window {allowed_ms} ms"
            ),
            Self::WrongLanguage { expected, detected } => {
                write!(f, "expected language {expected}, detected {detected}")
            }
            Self::BannedFiller(text) => write!(f, "content-free filler: {text}"),
            Self::MissingEvidence => write!(f, "no evidence ids cited"),
            Self::UnknownEvidence(id) => write!(f, "unknown evidence id: {id}"),
            Self::UnsupportedClaim(claim) => write!(f, "unsupported claim: {claim}"),
            Self::Repeated => write!(f, "repeats the adjacent narration line"),
        }
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_FIELD_CHARS)
        .collect()
}

fn join_tags(tags: &[String]) -> String {
    tags.iter()
        .map(|tag| sanitize(tag))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn request_from_context(
    context: &super::super::GapContext,
    gap_start_ms: u64,
    gap_end_ms: u64,
    language: &str,
    style: &str,
    max_words: usize,
    words_per_minute: f64,
) -> NarrationRequest {
    let mut acoustic_evidence = vec![EvidenceItem {
        id: "gap.duration".to_string(),
        description: format!("the gap is speech-free for {} ms", context.gap_duration_ms),
    }];
    if !context.self_tags.is_empty() {
        acoustic_evidence.push(EvidenceItem {
            id: "gap.tags".to_string(),
            description: format!(
                "audio tags detected inside the gap: {}",
                join_tags(&context.self_tags)
            ),
        });
    }
    if !context.gap_reason.trim().is_empty() {
        acoustic_evidence.push(EvidenceItem {
            id: "gap.reason".to_string(),
            description: sanitize(&context.gap_reason),
        });
    }
    if let Some(trend) = context.trend {
        acoustic_evidence.push(EvidenceItem {
            id: "gap.loudness".to_string(),
            description: format!("measured loudness trend: {trend:?}"),
        });
    }
    let mut adjacent_context = AdjacentContext::default();
    if !context.before_tags.is_empty() {
        adjacent_context.before = Some(segment_description(
            "before",
            context.before_kind.as_ref(),
            &context.before_tags,
        ));
        acoustic_evidence.push(EvidenceItem {
            id: "context.before".to_string(),
            description: adjacent_context.before.clone().unwrap_or_default(),
        });
    }
    if !context.after_tags.is_empty() {
        adjacent_context.after = Some(segment_description(
            "after",
            context.after_kind.as_ref(),
            &context.after_tags,
        ));
        acoustic_evidence.push(EvidenceItem {
            id: "context.after".to_string(),
            description: adjacent_context.after.clone().unwrap_or_default(),
        });
    }
    let forbidden_claims: Vec<String> = VISUAL_CLAIM_MARKERS
        .iter()
        .map(|marker| (*marker).to_string())
        .collect();
    NarrationRequest {
        prompt_version: super::PROMPT_VERSION.to_string(),
        gap_start_ms,
        gap_end_ms,
        allowed_duration_ms: context.gap_duration_ms,
        words_per_minute,
        max_words,
        language: language.to_string(),
        style: style.to_string(),
        acoustic_evidence,
        adjacent_context,
        visual_evidence: Vec::new(),
        forbidden_claims,
        forbidden_filler: super::super::lang::phrases(language)
            .banned_filler
            .iter()
            .map(|filler| (*filler).to_string())
            .collect(),
    }
}

fn segment_description(
    position: &str,
    kind: Option<&movie_radio_types::SegmentKind>,
    tags: &[String],
) -> String {
    format!(
        "{position} segment ({}) with tags: {}",
        kind.map_or_else(|| "unknown".to_string(), |kind| format!("{kind:?}")),
        join_tags(tags)
    )
}

/// Serializes the request as JSON inside the versioned template. JSON escaping
/// keeps media-derived strings inert: template delimiters and instruction-like
/// text can only appear inside quoted JSON values.
pub fn render_gap_prompt(request: &NarrationRequest) -> String {
    let json = serde_json::to_string_pretty(request).unwrap_or_else(|_| "{}".to_string());
    PROMPT_TEMPLATE.replace("{{REQUEST_JSON}}", &json)
}

pub fn parse_response(raw: &str) -> std::result::Result<NarrationResponse, NarrationRejection> {
    let trimmed = raw.trim();
    let unfenced = if let Some(rest) = trimmed.strip_prefix("```") {
        rest.strip_prefix("json").unwrap_or(rest).trim()
    } else {
        trimmed
    };
    let unfenced = unfenced.trim_end_matches("```").trim();
    let (Some(start), Some(end)) = (unfenced.find('{'), unfenced.rfind('}')) else {
        return Err(NarrationRejection::InvalidJson(
            "no JSON object found".to_string(),
        ));
    };
    if end < start {
        return Err(NarrationRejection::InvalidJson(
            "unbalanced JSON object".to_string(),
        ));
    }
    serde_json::from_str(&unfenced[start..=end])
        .map_err(|error| NarrationRejection::InvalidJson(error.to_string()))
}

pub fn validate_response(
    request: &NarrationRequest,
    response: &NarrationResponse,
    previous_text: Option<&str>,
) -> std::result::Result<(), NarrationRejection> {
    let text = response.text.trim();
    if text.is_empty() {
        return Err(NarrationRejection::Empty);
    }
    let words = text.split_whitespace().count();
    if words == 0 {
        return Err(NarrationRejection::Empty);
    }
    if words > request.max_words {
        return Err(NarrationRejection::TooManyWords {
            words,
            max: request.max_words,
        });
    }
    let estimated_ms = (words as f64 / request.words_per_minute * 60_000.0) as u64;
    if estimated_ms > request.allowed_duration_ms {
        return Err(NarrationRejection::OverBudget {
            estimated_ms,
            allowed_ms: request.allowed_duration_ms,
        });
    }
    if let Some(expected) = language_family(&request.language) {
        if let Some(detected) = detect_language(text) {
            if detected != expected {
                return Err(NarrationRejection::WrongLanguage {
                    expected: expected.to_string(),
                    detected: detected.to_string(),
                });
            }
        }
    }
    let normalized = normalize_filler(text);
    if request
        .forbidden_filler
        .iter()
        .any(|filler| normalize_filler(filler) == normalized)
    {
        return Err(NarrationRejection::BannedFiller(text.to_string()));
    }
    if response.evidence_ids.is_empty() {
        return Err(NarrationRejection::MissingEvidence);
    }
    let known: Vec<&str> = request
        .acoustic_evidence
        .iter()
        .chain(request.visual_evidence.iter())
        .map(|item| item.id.as_str())
        .collect();
    for id in &response.evidence_ids {
        if !known.contains(&id.as_str()) {
            return Err(NarrationRejection::UnknownEvidence(id.clone()));
        }
    }
    let lower = text.to_lowercase();
    if let Some(claim) = request
        .forbidden_claims
        .iter()
        .find(|claim| !claim.trim().is_empty() && lower.contains(&claim.to_lowercase()))
    {
        return Err(NarrationRejection::UnsupportedClaim(claim.clone()));
    }
    if previous_text.is_some_and(|previous| previous.trim().eq_ignore_ascii_case(text)) {
        return Err(NarrationRejection::Repeated);
    }
    Ok(())
}

fn normalize_filler(text: &str) -> String {
    text.trim()
        .trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_lowercase()
}

fn language_family(code: &str) -> Option<&'static str> {
    let code = code.trim().to_ascii_lowercase();
    if code.starts_with("de") {
        Some("de")
    } else if code.starts_with("en") {
        Some("en")
    } else {
        None
    }
}

fn detect_language(text: &str) -> Option<&'static str> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    let german = words
        .iter()
        .filter(|word| GERMAN_MARKERS.contains(&word.as_str()))
        .count();
    let english = words
        .iter()
        .filter(|word| ENGLISH_MARKERS.contains(&word.as_str()))
        .count();
    if german >= 2 && german > english {
        Some("de")
    } else if english >= 2 && english > german {
        Some("en")
    } else {
        None
    }
}
