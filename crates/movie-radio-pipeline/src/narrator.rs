use anyhow::{bail, Result};
use movie_radio_types::{parse_narrator_style, NarratorConfig, NarratorParams, RenderedPrompt};
use std::path::PathBuf;
use tera::{Context, Tera};

#[async_trait::async_trait]
pub trait NarratorAiBackend: Send + Sync {
    async fn generate_narration(
        &self,
        prompt: &RenderedPrompt,
        params: &NarratorParams,
    ) -> Result<String>;
}

pub struct RenderData {
    pub movie_title: String,
    pub prev_scene: String,
    pub scene_type: String,
    pub duration_secs: u32,
    pub visual_description: String,
    pub characters: String,
    pub mood: String,
    pub language: String,
    pub max_words: u32,
}

pub fn render_prompt(template_text: &str, data: &RenderData) -> Result<RenderedPrompt> {
    let mut tera = Tera::default();
    tera.add_raw_template("prompt", template_text)
        .map_err(|e| anyhow::anyhow!("tera parse: {e}"))?;
    let mut ctx = Context::new();
    ctx.insert("movie_title", &data.movie_title);
    ctx.insert("prev_scene", &data.prev_scene);
    ctx.insert("scene_type", &data.scene_type);
    ctx.insert("duration_secs", &data.duration_secs);
    ctx.insert("visual_description", &data.visual_description);
    ctx.insert("characters", &data.characters);
    ctx.insert("mood", &data.mood);
    ctx.insert("language", &data.language);
    ctx.insert("max_words", &data.max_words);
    let rendered = tera
        .render("prompt", &ctx)
        .map_err(|e| anyhow::anyhow!("tera render: {e}"))?;
    let text = strip_frontmatter(&rendered);
    Ok(RenderedPrompt {
        text,
        variables: std::collections::HashMap::new(),
    })
}

fn strip_frontmatter(s: &str) -> String {
    let trimmed = s.trim_start();
    if let Some(stripped) = trimmed.strip_prefix("---") {
        if let Some(end) = stripped.find("---") {
            return stripped[end + 3..].trim().to_string();
        }
    }
    s.trim().to_string()
}

pub struct OpenAiNarrator {
    base_url: String,
    model: String,
    api_key_env: String,
}

impl OpenAiNarrator {
    pub fn new(cfg: &NarratorConfig) -> Self {
        Self {
            base_url: cfg.openai.base_url.clone(),
            model: cfg.openai.model.clone(),
            api_key_env: cfg.openai.api_key_env.clone(),
        }
    }
}

#[async_trait::async_trait]
impl NarratorAiBackend for OpenAiNarrator {
    async fn generate_narration(
        &self,
        prompt: &RenderedPrompt,
        _params: &NarratorParams,
    ) -> Result<String> {
        // Never log secrets: api key fetched at runtime, redacted on error.
        let key = std::env::var(&self.api_key_env).unwrap_or_default();
        if key.is_empty() {
            bail!("missing api key env {}", self.api_key_env);
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({"model": self.model, "messages": [{"role": "user", "content": prompt.text}], "max_tokens": 200});
        let resp = client
            .post(&url)
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("openai request failed: {e}"))?;
        if !resp.status().is_success() {
            bail!("openai http {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(text)
    }
}

pub struct OllamaLocalNarrator {
    base_url: String,
    model: String,
}

impl OllamaLocalNarrator {
    pub fn new(cfg: &NarratorConfig) -> Self {
        Self {
            base_url: cfg.ollama_local.base_url.clone(),
            model: cfg.ollama_local.model.clone(),
        }
    }
}

#[async_trait::async_trait]
impl NarratorAiBackend for OllamaLocalNarrator {
    async fn generate_narration(
        &self,
        prompt: &RenderedPrompt,
        _params: &NarratorParams,
    ) -> Result<String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        let url = format!("{}/api/generate", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({"model": self.model, "prompt": prompt.text, "stream": false});
        let resp = client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            bail!("ollama http {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await?;
        Ok(v["response"].as_str().unwrap_or("").to_string())
    }
}

pub struct AnthropicNarrator {
    api_key_env: String,
    model: String,
}

impl AnthropicNarrator {
    pub fn new(cfg: &NarratorConfig) -> Self {
        Self {
            api_key_env: cfg.anthropic.api_key_env.clone(),
            model: cfg.anthropic.model.clone(),
        }
    }
}

#[async_trait::async_trait]
impl NarratorAiBackend for AnthropicNarrator {
    async fn generate_narration(
        &self,
        prompt: &RenderedPrompt,
        _params: &NarratorParams,
    ) -> Result<String> {
        let key = std::env::var(&self.api_key_env).unwrap_or_default();
        if key.is_empty() {
            bail!("missing api key env {}", self.api_key_env);
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let body = serde_json::json!({"model": self.model, "max_tokens": 200, "messages": [{"role": "user", "content": prompt.text}]});
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("anthropic http {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await?;
        Ok(v["content"][0]["text"].as_str().unwrap_or("").to_string())
    }
}

pub fn handle_narrate(
    scene: Option<u32>,
    dry_run: bool,
    template: Option<PathBuf>,
    cfg: &NarratorConfig,
) -> Result<()> {
    let tpl_path = template.unwrap_or_else(|| PathBuf::from(&cfg.prompt_template));
    let tpl_text = std::fs::read_to_string(&tpl_path)
        .unwrap_or_else(|_| "Write narration for {{movie_title}} in {{language}}".to_string());
    let data = RenderData {
        movie_title: "Example Movie".to_string(),
        prev_scene: "Previous summary".to_string(),
        scene_type: "dialogue".to_string(),
        duration_secs: 30,
        visual_description: "A quiet scene".to_string(),
        characters: "protagonist".to_string(),
        mood: "mysterious".to_string(),
        language: cfg.language.clone(),
        max_words: 50,
    };
    let rendered = render_prompt(&tpl_text, &data)?;
    if dry_run || scene.is_some() {
        println!(
            "--- rendered prompt (language={}, style={}, backend={}) ---",
            cfg.language, cfg.style, cfg.backend
        );
        println!("{}", rendered.text);
        return Ok(());
    }
    // Real generation would dispatch to backend based on cfg.backend
    let style = parse_narrator_style(&cfg.style);
    let params = NarratorParams {
        language: cfg.language.clone(),
        style,
        max_tokens: cfg.max_tokens,
        temperature: cfg.temperature,
        ..Default::default()
    };
    println!(
        "narrate dry-run language={} params={:?}",
        params.language, params.style
    );
    println!("{}", rendered.text);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_prompt_basic() {
        let tpl = "Hello {{ movie_title }} in {{ language }}";
        let data = RenderData {
            movie_title: "Test".to_string(),
            prev_scene: "".to_string(),
            scene_type: "".to_string(),
            duration_secs: 10,
            visual_description: "".to_string(),
            characters: "".to_string(),
            mood: "".to_string(),
            language: "en-US".to_string(),
            max_words: 10,
        };
        let r = render_prompt(tpl, &data).unwrap();
        assert!(r.text.contains("Test"));
    }

    #[test]
    fn strip_frontmatter_handles() {
        let s = "---\nstyle: radio_drama\n---\nHello world";
        assert_eq!(strip_frontmatter(s), "Hello world");
    }
}
