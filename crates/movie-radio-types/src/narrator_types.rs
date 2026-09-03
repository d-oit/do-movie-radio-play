use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NarratorStyle {
    #[default]
    RadioDrama,
    Documentary,
    Cinematic,
    Custom(String),
}

pub fn parse_narrator_style(s: &str) -> NarratorStyle {
    match s {
        "radio_drama" => NarratorStyle::RadioDrama,
        "documentary" => NarratorStyle::Documentary,
        "cinematic" => NarratorStyle::Cinematic,
        other => NarratorStyle::Custom(other.to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarratorParams {
    pub language: String,
    pub style: NarratorStyle,
    pub max_tokens: u32,
    pub temperature: f32,
    pub model: Option<String>,
    pub system_prompt_override: Option<String>,
}

impl Default for NarratorParams {
    fn default() -> Self {
        Self {
            language: "en-US".to_string(),
            style: NarratorStyle::RadioDrama,
            max_tokens: 200,
            temperature: 0.7,
            model: None,
            system_prompt_override: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RenderedPrompt {
    pub text: String,
    pub variables: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_parsing() {
        assert_eq!(
            parse_narrator_style("radio_drama"),
            NarratorStyle::RadioDrama
        );
        assert_eq!(
            parse_narrator_style("custom_x"),
            NarratorStyle::Custom("custom_x".to_string())
        );
    }
}
