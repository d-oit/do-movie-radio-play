//! Separates observed visual facts from inferred interpretation.
//!
//! Anything a frame cannot prove (dialogue attribution, emotion, causality,
//! unsupported identities, "we see" camera language, low confidence) is
//! rejected and recorded, so it can never reach the narration text.

use serde::{Deserialize, Serialize};

use super::provider::{FrameObservation, VisionAnalysis};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundingIssue {
    DialogueAttribution,
    EmotionClaim,
    CausalClaim,
    UnsupportedIdentity,
    VisualOnlyLanguage,
    EmptyDescription,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedClaim {
    pub text: String,
    pub reason: GroundingIssue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroundedFrame {
    pub frame_ref: String,
    pub timestamp_ms: u64,
    pub confidence: f32,
    pub sentences: Vec<String>,
    pub entities: Vec<String>,
    pub actions: Vec<String>,
    pub uncertainty: Vec<String>,
    pub rejected: Vec<RejectedClaim>,
    /// False when the frame is below the confidence threshold or nothing
    /// observable survived grounding.
    pub usable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroundingPolicy {
    pub known_characters: Vec<String>,
    pub min_confidence_percent: u8,
}

impl GroundingPolicy {
    pub fn new(known_characters: Vec<String>, min_confidence: f32) -> Self {
        Self {
            known_characters,
            min_confidence_percent: (min_confidence.clamp(0.0, 1.0) * 100.0) as u8,
        }
    }

    fn min_confidence(&self) -> f32 {
        f32::from(self.min_confidence_percent) / 100.0
    }
}

const DIALOGUE_MARKERS: &[&str] = &[
    " sagt",
    " sagt:",
    " spricht",
    " flüstert",
    " ruft",
    " antwortet",
    " erzählt",
    " schreit",
    " murmelt",
    " says",
    " asks",
    " replies",
    " whispers",
    " shouts",
    " tells",
    " speaks",
];

const EMOTION_MARKERS: &[&str] = &[
    "traurig",
    "wütend",
    "ärgerlich",
    "ängstlich",
    "verzweifelt",
    "glücklich",
    "fröhlich",
    "aufgeregt",
    "überrascht",
    "verliebt",
    "sad",
    "angry",
    "furious",
    "afraid",
    "scared",
    "happy",
    "joyful",
    "excited",
    "surprised",
    "terrified",
    "anxious",
];

const CAUSAL_MARKERS: &[&str] = &[
    " weil ",
    " deshalb",
    " darum",
    " deswegen",
    " damit ",
    "because",
    "therefore",
    "thus",
    "so that",
    "in order to",
];

const VISUAL_ONLY_MARKERS: &[&str] = &[
    "we see",
    "the camera",
    "we watch",
    "wir sehen",
    "die kamera",
    "im bild",
];

const NAMING_MARKERS: &[&str] = &["heißt", "namens", "called", "named"];

/// Grounds each frame observation, dropping unsupported sentences and terms.
pub fn ground(analysis: &VisionAnalysis, policy: &GroundingPolicy) -> Vec<GroundedFrame> {
    analysis
        .frames
        .iter()
        .map(|observation| ground_observation(observation, policy))
        .collect()
}

fn ground_observation(observation: &FrameObservation, policy: &GroundingPolicy) -> GroundedFrame {
    let mut rejected = Vec::new();
    let mut sentences = Vec::new();
    for sentence in split_sentences(&observation.description) {
        match classify(&sentence, policy) {
            Some(reason) => rejected.push(RejectedClaim {
                text: sentence,
                reason,
            }),
            None => sentences.push(sentence),
        }
    }
    if sentences.is_empty() && observation.description.trim().is_empty() {
        rejected.push(RejectedClaim {
            text: String::new(),
            reason: GroundingIssue::EmptyDescription,
        });
    }
    let entities = keep_grounded(&observation.entities, policy, &mut rejected);
    let actions = keep_grounded(&observation.actions, policy, &mut rejected);

    let mut uncertainty = Vec::new();
    if let Some(note) = observation
        .uncertainty
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        uncertainty.push(note.to_string());
    }
    let low_confidence = observation.confidence < policy.min_confidence();
    if low_confidence {
        uncertainty.push(format!(
            "frame confidence {:.2} below {:.2}",
            observation.confidence,
            policy.min_confidence()
        ));
    }
    let usable = !low_confidence && !sentences.is_empty();

    GroundedFrame {
        frame_ref: observation.frame_ref.clone(),
        timestamp_ms: observation.timestamp_ms,
        confidence: observation.confidence,
        sentences,
        entities,
        actions,
        uncertainty,
        usable,
        rejected,
    }
}

fn keep_grounded(
    terms: &[String],
    policy: &GroundingPolicy,
    rejected: &mut Vec<RejectedClaim>,
) -> Vec<String> {
    let mut kept = Vec::new();
    for term in terms {
        match classify(term, policy) {
            Some(reason) => rejected.push(RejectedClaim {
                text: term.clone(),
                reason,
            }),
            None if !term.trim().is_empty() => kept.push(term.trim().to_string()),
            None => {}
        }
    }
    kept
}

/// Returns why a single claim is not observable, or `None` when it is safe.
pub fn classify(claim: &str, policy: &GroundingPolicy) -> Option<GroundingIssue> {
    let lower = claim.to_lowercase();
    if contains_marker(&lower, DIALOGUE_MARKERS) {
        return Some(GroundingIssue::DialogueAttribution);
    }
    if contains_marker(&lower, EMOTION_MARKERS) {
        return Some(GroundingIssue::EmotionClaim);
    }
    if contains_marker(&lower, CAUSAL_MARKERS) {
        return Some(GroundingIssue::CausalClaim);
    }
    if contains_marker(&lower, VISUAL_ONLY_MARKERS) {
        return Some(GroundingIssue::VisualOnlyLanguage);
    }
    if names_unknown_person(claim, &lower, &policy.known_characters) {
        return Some(GroundingIssue::UnsupportedIdentity);
    }
    None
}

fn contains_marker(lower: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| lower.contains(marker))
}

fn names_unknown_person(claim: &str, lower: &str, known: &[String]) -> bool {
    let Some(cue) = NAMING_MARKERS.iter().find(|cue| lower.contains(**cue)) else {
        return false;
    };
    let Some(position) = lower.find(*cue) else {
        return false;
    };
    let tail = &claim[position + cue.len()..];
    // A name is the capitalised run directly after the cue; German nouns later
    // in the sentence are capitalised too and must not be read as names.
    let names: Vec<String> = tail
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .take_while(|word| word.chars().next().is_some_and(char::is_uppercase))
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    if names.is_empty() {
        // Claims an identity without naming anyone: still unverifiable.
        return true;
    }
    names
        .iter()
        .any(|name| !known.iter().any(|known| known.eq_ignore_ascii_case(name)))
}

/// Splits a description into whole sentences, keeping terminal punctuation.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            push_sentence(&mut sentences, &mut current);
        }
    }
    push_sentence(&mut sentences, &mut current);
    sentences
}

fn push_sentence(sentences: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        sentences.push(trimmed.to_string());
    }
    current.clear();
}
