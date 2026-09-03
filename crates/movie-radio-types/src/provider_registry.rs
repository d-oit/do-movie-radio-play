use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::compute::{ComputeEndpoint, ExecutionLocation};
use crate::config::GpuPolicyConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderEntry {
    pub capability: String,
    pub provider_id: String,
    pub execution: ExecutionLocation,
    pub endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    entries: HashMap<String, ProviderEntry>,
    endpoints: HashMap<String, ComputeEndpoint>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            endpoints: HashMap::new(),
        }
    }

    pub fn register_provider(&mut self, entry: ProviderEntry) -> Result<(), String> {
        if entry.capability.trim().is_empty() {
            return Err("capability must not be empty".to_string());
        }
        if entry.provider_id.trim().is_empty() {
            return Err("provider_id must not be empty".to_string());
        }
        if entry.provider_id == "xtts_local"
            || entry.provider_id == "coqui_local"
            || entry.provider_id == "kokoro_local"
        {
            return Err(format!(
                "provider {} encodes compute location; use audio_cpp with execution location",
                entry.provider_id
            ));
        }
        self.entries.insert(entry.capability.clone(), entry);
        Ok(())
    }

    pub fn register_endpoint(&mut self, endpoint: ComputeEndpoint) -> Result<(), String> {
        endpoint.validate()?;
        self.endpoints.insert(endpoint.id.clone(), endpoint);
        Ok(())
    }

    pub fn resolve(&self, capability: &str) -> Option<&ProviderEntry> {
        self.entries.get(capability)
    }

    pub fn select_endpoint(
        &self,
        policy: &GpuPolicyConfig,
    ) -> Result<Option<&ComputeEndpoint>, String> {
        let mut candidates: Vec<&ComputeEndpoint> = self.endpoints.values().collect();
        candidates.sort_by_key(|e| e.priority);
        if policy.prefer_free {
            if let Some(free) = candidates.iter().find(|e| e.is_free()) {
                return Ok(Some(*free));
            }
        }
        if policy.allow_paid {
            return Ok(candidates.into_iter().next());
        }
        let free_only: Vec<&&ComputeEndpoint> = candidates.iter().filter(|e| e.is_free()).collect();
        if let Some(first) = free_only.first() {
            return Ok(Some(**first));
        }
        if candidates.is_empty() {
            return Ok(None);
        }
        Err("no permitted endpoint: paid execution requires explicit opt-in".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_endpoint() -> ComputeEndpoint {
        ComputeEndpoint {
            id: "free".to_string(),
            url: Some("https://gpu-free.example.com".to_string()),
            cost_per_hour: Some(0.0),
            priority: 10,
        }
    }

    #[test]
    fn rejects_location_encoded_provider() {
        let mut reg = ProviderRegistry::default();
        let entry = ProviderEntry {
            capability: "tts".to_string(),
            provider_id: "xtts_local".to_string(),
            execution: ExecutionLocation::Local,
            endpoint_id: None,
        };
        assert!(reg.register_provider(entry).is_err());
    }

    #[test]
    fn prefers_free_endpoint() {
        let mut reg = ProviderRegistry::default();
        assert!(reg.register_endpoint(free_endpoint()).is_ok());
        assert!(reg
            .register_endpoint(ComputeEndpoint {
                id: "paid".to_string(),
                url: Some("https://gpu-paid.example.com".to_string()),
                cost_per_hour: Some(0.4),
                priority: 20,
            })
            .is_ok());
        let policy = GpuPolicyConfig {
            prefer_free: true,
            allow_paid: false,
            max_cost_per_job: 0.5,
            max_cost_per_day: 5.0,
        };
        let selected = reg.select_endpoint(&policy).unwrap_or(None);
        assert!(selected.is_some());
        assert_eq!(selected.map(|e| e.id.as_str()), Some("free"));
    }

    #[test]
    fn paid_requires_opt_in() {
        let mut reg = ProviderRegistry::default();
        assert!(reg
            .register_endpoint(ComputeEndpoint {
                id: "paid".to_string(),
                url: Some("https://gpu-paid.example.com".to_string()),
                cost_per_hour: Some(0.4),
                priority: 20,
            })
            .is_ok());
        let policy = GpuPolicyConfig {
            prefer_free: true,
            allow_paid: false,
            max_cost_per_job: 0.5,
            max_cost_per_day: 5.0,
        };
        assert!(reg.select_endpoint(&policy).is_err());
    }
}
