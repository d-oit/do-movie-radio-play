pub mod ai_generate;
pub mod freesound;
pub mod local;
pub mod manager;
pub mod processor;

use anyhow::Result;
use async_trait::async_trait;
use movie_radio_types::{SfxCandidate, SfxProviderCapabilities, SfxQuery};

#[async_trait]
pub trait SoundEffectBackend: Send + Sync {
    async fn search(&self, query: &SfxQuery) -> Result<Vec<SfxCandidate>>;
    async fn fetch(&self, candidate: &SfxCandidate) -> Result<Vec<u8>>;
    fn capabilities(&self) -> SfxProviderCapabilities;
}

pub use ai_generate::AiGenerateSfxBackend;
pub use freesound::FreesoundBackend;
pub use local::LocalSfxBackend;
pub use manager::SfxManager;
pub use processor::SfxProcessor;
