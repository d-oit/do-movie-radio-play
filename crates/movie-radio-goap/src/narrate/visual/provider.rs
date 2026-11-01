//! Provider-agnostic vision interface plus structured-output validation.
//!
//! Providers only return raw text; [`parse_analysis`] turns it into structured
//! observations and rejects output that does not reference a supplied frame.

use std::fmt;
use std::time::Duration;

use base64::Engine as _;
use movie_radio_types::VisualConfig;
use serde::{Deserialize, Serialize};

use super::frames::FrameSample;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionErrorKind {
    MissingCredentials,
    Transport,
    MalformedOutput,
    UnknownFrame,
}

#[derive(Debug, Clone)]
pub struct VisionError {
    pub kind: VisionErrorKind,
    pub detail: String,
}

impl VisionError {
    pub fn new(kind: VisionErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    fn malformed(detail: impl Into<String>) -> Self {
        Self::new(VisionErrorKind::MalformedOutput, detail)
    }
}

impl fmt::Display for VisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for VisionError {}

/// One frame's structured observation. `timestamp_ms` always comes from the
/// supplied frame, never from model output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameObservation {
    #[serde(default)]
    pub frame_ref: String,
    #[serde(default)]
    pub timestamp_ms: u64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub uncertainty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VisionAnalysis {
    #[serde(default)]
    pub frames: Vec<FrameObservation>,
}

pub struct VisionRequest<'a> {
    pub language: &'a str,
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub max_words: usize,
    pub acoustic_tags: &'a [String],
    pub before_context: &'a str,
    pub after_context: &'a str,
    pub known_characters: &'a [String],
    pub frames: &'a [FrameSample],
}

#[async_trait::async_trait]
pub trait VisionProvider: Send + Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    async fn describe(&self, request: &VisionRequest<'_>) -> Result<String, VisionError>;
}

/// Any OpenAI-compatible `/chat/completions` vision endpoint.
pub struct OpenAiVision {
    base_url: String,
    model: String,
    api_key_env: String,
    timeout: Duration,
}

impl OpenAiVision {
    pub fn from_config(cfg: &VisualConfig) -> Self {
        Self {
            base_url: cfg.base_url.clone(),
            model: cfg.model.clone(),
            api_key_env: cfg.api_key_env.clone(),
            timeout: Duration::from_secs(cfg.timeout_secs.max(1)),
        }
    }
}

#[async_trait::async_trait]
impl VisionProvider for OpenAiVision {
    fn name(&self) -> &str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn describe(&self, request: &VisionRequest<'_>) -> Result<String, VisionError> {
        let key = std::env::var(&self.api_key_env).unwrap_or_default();
        if key.is_empty() {
            return Err(VisionError::new(
                VisionErrorKind::MissingCredentials,
                format!("missing api key env {}", self.api_key_env),
            ));
        }
        let mut content = vec![serde_json::json!({
            "type": "text",
            "text": build_prompt(request),
        })];
        for frame in request.frames {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&frame.bytes);
            content.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": format!("data:image/jpeg;base64,{encoded}") },
            }));
        }
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": content }],
            "max_tokens": 600,
            "temperature": 0,
        });
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| VisionError::new(VisionErrorKind::Transport, e.to_string()))?;
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = client
            .post(url)
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .map_err(|e| VisionError::new(VisionErrorKind::Transport, e.to_string()))?;
        if !response.status().is_success() {
            return Err(VisionError::new(
                VisionErrorKind::Transport,
                format!("openai vision http {}", response.status()),
            ));
        }
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| VisionError::new(VisionErrorKind::MalformedOutput, e.to_string()))?;
        value["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| VisionError::malformed("openai response had no message content"))
    }
}

/// Local/offline vision models served by Ollama (`llava`, `qwen2.5-vl`, ...).
pub struct OllamaVision {
    base_url: String,
    model: String,
    timeout: Duration,
}

impl OllamaVision {
    pub fn from_config(cfg: &VisualConfig) -> Self {
        Self {
            base_url: cfg.base_url.clone(),
            model: cfg.model.clone(),
            timeout: Duration::from_secs(cfg.timeout_secs.max(1)),
        }
    }
}

#[async_trait::async_trait]
impl VisionProvider for OllamaVision {
    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn describe(&self, request: &VisionRequest<'_>) -> Result<String, VisionError> {
        let images: Vec<String> = request
            .frames
            .iter()
            .map(|f| base64::engine::general_purpose::STANDARD.encode(&f.bytes))
            .collect();
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "messages": [{
                "role": "user",
                "content": build_prompt(request),
                "images": images,
            }],
        });
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| VisionError::new(VisionErrorKind::Transport, e.to_string()))?;
        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let response = client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| VisionError::new(VisionErrorKind::Transport, e.to_string()))?;
        if !response.status().is_success() {
            return Err(VisionError::new(
                VisionErrorKind::Transport,
                format!("ollama vision http {}", response.status()),
            ));
        }
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| VisionError::new(VisionErrorKind::MalformedOutput, e.to_string()))?;
        value["message"]["content"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| VisionError::malformed("ollama response had no message content"))
    }
}

/// Prompt shared by every provider: only observed facts, evidence references
/// and explicit uncertainty.
pub fn build_prompt(request: &VisionRequest<'_>) -> String {
    let tags = if request.acoustic_tags.is_empty() {
        "none".to_string()
    } else {
        request.acoustic_tags.join(", ")
    };
    let known = if request.known_characters.is_empty() {
        "none supplied".to_string()
    } else {
        request.known_characters.join(", ")
    };
    let frames = request
        .frames
        .iter()
        .map(|f| format!("{} at {} ms", f.frame_ref, f.timestamp_ms))
        .collect::<Vec<_>>()
        .join("\n- ");
    format!(
        "You are a professional audio-description analyst for a radio-play pipeline.\n\
         Window: {start} ms to {end} ms. Narration language: {language}. Word budget: {words}.\n\
         Acoustic tags already detected in this window: {tags}.\n\
         Before: {before}\nAfter: {after}\nKnown characters: {known}\n\
         Representative frames (use these references exactly):\n- {frames}\n\n\
         Describe only what is directly visible: entities, actions, positions and scene changes.\n\
         Use present tense, third person and factual wording. Never write \"we see\" or \"the camera\".\n\
         Never attribute dialogue, emotions, motives or causes. Never name a character unless the\n\
         name is listed as a known character. If a frame is dark, obstructed or ambiguous, lower the\n\
         confidence and state the uncertainty instead of guessing.\n\
         Return JSON only, no prose: {{\"frames\":[{{\"frame_ref\":\"...\",\"timestamp_ms\":0,\
         \"description\":\"...\",\"entities\":[\"...\"],\"actions\":[\"...\"],\"confidence\":0.0,\
         \"uncertainty\":\"...\"}}]}}",
        start = request.window_start_ms,
        end = request.window_end_ms,
        language = request.language,
        words = request.max_words,
        tags = tags,
        before = request.before_context,
        after = request.after_context,
        known = known,
        frames = frames,
    )
}

/// Parses and validates a provider response. Tolerates Markdown code fences
/// and either a `{"frames": [...]}` object or a bare array.
pub fn parse_analysis(raw: &str, frames: &[FrameSample]) -> Result<VisionAnalysis, VisionError> {
    let json = extract_json(raw)
        .ok_or_else(|| VisionError::malformed("provider output contained no JSON object"))?;
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| VisionError::malformed(format!("invalid JSON: {e}")))?;
    let items = match value.get("frames") {
        Some(serde_json::Value::Array(items)) => items.clone(),
        Some(other @ serde_json::Value::Object(_)) => vec![other.clone()],
        Some(_) => {
            return Err(VisionError::malformed(
                "frames field must be an array or object",
            ))
        }
        None => match value {
            serde_json::Value::Array(items) => items,
            other @ serde_json::Value::Object(_) => vec![other],
            _ => return Err(VisionError::malformed("expected JSON object or array")),
        },
    };
    if items.is_empty() {
        return Err(VisionError::malformed(
            "provider returned no frame observations",
        ));
    }

    let mut observations = Vec::with_capacity(items.len());
    for item in items {
        let mut observation: FrameObservation = serde_json::from_value(item)
            .map_err(|e| VisionError::malformed(format!("invalid observation: {e}")))?;
        let frame = match_frame(frames, &observation).ok_or_else(|| {
            VisionError::new(
                VisionErrorKind::UnknownFrame,
                format!(
                    "observation referenced unknown frame '{}' / timestamp {}",
                    observation.frame_ref, observation.timestamp_ms
                ),
            )
        })?;
        observation.frame_ref.clone_from(&frame.frame_ref);
        observation.timestamp_ms = frame.timestamp_ms;
        if !observation.confidence.is_finite() {
            return Err(VisionError::malformed("confidence is not a finite number"));
        }
        observation.confidence = observation.confidence.clamp(0.0, 1.0);
        observations.push(observation);
    }
    Ok(VisionAnalysis {
        frames: observations,
    })
}

fn match_frame<'a>(
    frames: &'a [FrameSample],
    observation: &FrameObservation,
) -> Option<&'a FrameSample> {
    if let Some(frame) = frames.iter().find(|f| f.frame_ref == observation.frame_ref) {
        return Some(frame);
    }
    frames
        .iter()
        .min_by_key(|f| f.timestamp_ms.abs_diff(observation.timestamp_ms))
        .filter(|f| f.timestamp_ms.abs_diff(observation.timestamp_ms) <= 500)
}

fn extract_json(raw: &str) -> Option<&str> {
    let start = raw.find('{').or_else(|| raw.find('['))?;
    let end = raw.rfind('}').or_else(|| raw.rfind(']'))?;
    (end > start).then(|| &raw[start..=end])
}
