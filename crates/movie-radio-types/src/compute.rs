use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionLocation {
    #[default]
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeEndpoint {
    pub id: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub cost_per_hour: Option<f64>,
    #[serde(default)]
    pub priority: u32,
}

impl ComputeEndpoint {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("compute endpoint id must not be empty".to_string());
        }
        if let Some(url) = &self.url {
            if !url.is_empty() {
                let parsed =
                    url::Url::parse(url).map_err(|e| format!("invalid endpoint url: {e}"))?;
                if parsed.scheme() != "http" && parsed.scheme() != "https" {
                    return Err(format!("endpoint url must be http or https: {url}"));
                }
            }
        }
        if let Some(cost) = self.cost_per_hour {
            if cost < 0.0 || !cost.is_finite() {
                return Err(format!("invalid cost_per_hour: {cost}"));
            }
        }
        Ok(())
    }

    pub fn is_free(&self) -> bool {
        self.cost_per_hour.is_none_or(|c| c == 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_validation() {
        let ep = ComputeEndpoint {
            id: "free".to_string(),
            url: Some("https://gpu.example.com".to_string()),
            cost_per_hour: Some(0.0),
            priority: 10,
        };
        assert!(ep.validate().is_ok());
        assert!(ep.is_free());
    }

    #[test]
    fn empty_id_rejected() {
        let ep = ComputeEndpoint {
            id: "".to_string(),
            url: None,
            cost_per_hour: None,
            priority: 0,
        };
        assert!(ep.validate().is_err());
    }
}
