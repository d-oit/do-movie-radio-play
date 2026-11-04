//! Which external LLM/vision providers this project may call (ADR-131).
//!
//! Allowed: OpenRouter, a local or self-hosted Ollama, and merge.dev. Direct
//! OpenAI and Anthropic endpoints and keys are rejected, so a stray
//! `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` can never be used by the pipeline.
//! The check is an allowlist: an unknown host is refused rather than trusted.

/// Provider names accepted by the narrator and vision configuration.
pub const ALLOWED_PROVIDERS: [&str; 3] = ["openrouter", "ollama", "merge_dev"];

/// Key variables that must never be read by an LLM/vision call.
pub const FORBIDDEN_KEY_ENVS: [&str; 2] = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY"];

const FORBIDDEN_HOSTS: [&str; 2] = ["api.openai.com", "api.anthropic.com"];
const ALLOWED_HOSTS: [&str; 2] = ["openrouter.ai", "merge.dev"];
const LOCAL_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyViolation {
    /// The configured key variable is a direct provider key.
    ForbiddenKeyEnv(String),
    /// The endpoint is a direct provider API.
    ForbiddenHost(String),
    /// The endpoint is not one of the allowed providers.
    UnknownHost(String),
    /// The URL has no host at all.
    MalformedUrl(String),
}

impl std::fmt::Display for PolicyViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForbiddenKeyEnv(key) => write!(
                f,
                "{key} is a direct provider key; use OPENROUTER_API_KEY, a local Ollama, or merge.dev (ADR-131)"
            ),
            Self::ForbiddenHost(host) => write!(
                f,
                "{host} is a direct provider API; route through OpenRouter, a local Ollama, or merge.dev (ADR-131)"
            ),
            Self::UnknownHost(host) => write!(
                f,
                "{host} is not an allowed LLM provider; allowed: OpenRouter, Ollama, merge.dev (ADR-131)"
            ),
            Self::MalformedUrl(url) => write!(f, "cannot read a host from '{url}'"),
        }
    }
}

impl std::error::Error for PolicyViolation {}

/// Lowercased host of `url`, without scheme, userinfo or port.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split_once(']').map(|(host, _)| host)?
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

fn host_matches(host: &str, allowed: &str) -> bool {
    host == allowed || host.ends_with(&format!(".{allowed}"))
}

/// Rejects a direct provider key variable or an endpoint outside the allowlist.
///
/// `api_key_env` may be empty for keyless providers (Ollama).
pub fn check_endpoint(base_url: &str, api_key_env: &str) -> Result<(), PolicyViolation> {
    if FORBIDDEN_KEY_ENVS
        .iter()
        .any(|key| key.eq_ignore_ascii_case(api_key_env.trim()))
    {
        return Err(PolicyViolation::ForbiddenKeyEnv(
            api_key_env.trim().to_string(),
        ));
    }
    let host = host_of(base_url.trim())
        .ok_or_else(|| PolicyViolation::MalformedUrl(base_url.to_string()))?;
    if FORBIDDEN_HOSTS
        .iter()
        .any(|forbidden| host_matches(&host, forbidden))
    {
        return Err(PolicyViolation::ForbiddenHost(host));
    }
    if LOCAL_HOSTS.contains(&host.as_str())
        || ALLOWED_HOSTS
            .iter()
            .any(|allowed| host_matches(&host, allowed))
    {
        return Ok(());
    }
    Err(PolicyViolation::UnknownHost(host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_providers_pass() {
        for (url, key) in [
            ("https://openrouter.ai/api/v1", "OPENROUTER_API_KEY"),
            ("https://api.merge.dev/v1", "MERGE_DEV_API_KEY"),
            ("http://localhost:11434", ""),
            ("http://127.0.0.1:11434", ""),
        ] {
            assert_eq!(check_endpoint(url, key), Ok(()), "{url}");
        }
    }

    #[test]
    fn direct_provider_hosts_are_rejected() {
        for url in [
            "https://api.openai.com/v1",
            "https://API.OpenAI.com/v1/",
            "https://api.anthropic.com/v1",
        ] {
            assert!(
                matches!(
                    check_endpoint(url, "OPENROUTER_API_KEY"),
                    Err(PolicyViolation::ForbiddenHost(_))
                ),
                "{url}"
            );
        }
    }

    #[test]
    fn direct_provider_keys_are_rejected_even_on_an_allowed_host() {
        for key in ["OPENAI_API_KEY", "anthropic_api_key", " ANTHROPIC_API_KEY "] {
            assert!(matches!(
                check_endpoint("https://openrouter.ai/api/v1", key),
                Err(PolicyViolation::ForbiddenKeyEnv(_))
            ));
        }
    }

    #[test]
    fn unknown_hosts_and_malformed_urls_are_rejected() {
        assert!(matches!(
            check_endpoint("https://llm.example.com/v1", "SOME_KEY"),
            Err(PolicyViolation::UnknownHost(_))
        ));
        // A forbidden host hidden in the path is not the host.
        assert!(matches!(
            check_endpoint("https://evil.example/api.openai.com", "SOME_KEY"),
            Err(PolicyViolation::UnknownHost(_))
        ));
        assert!(matches!(
            check_endpoint("", ""),
            Err(PolicyViolation::MalformedUrl(_))
        ));
    }

    #[test]
    fn host_parsing_ignores_userinfo_port_and_case() {
        assert_eq!(
            host_of("https://user:pw@OpenRouter.AI:443/api/v1"),
            Some("openrouter.ai".to_string())
        );
        assert_eq!(host_of("http://[::1]:11434"), Some("::1".to_string()));
        assert_eq!(
            host_of("openrouter.ai/api"),
            Some("openrouter.ai".to_string())
        );
    }
}
