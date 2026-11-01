use movie_radio_types::NarratorConfig;

use super::VisualNarrationReport;

/// Visual-stage inputs and outputs carried through the GOAP pipeline.
#[derive(Debug, Default)]
pub struct VisualPipelineState {
    pub narrator_config: Option<NarratorConfig>,
    /// Character names a vision model may use; other identities are rejected.
    pub known_characters: Vec<String>,
    pub report: Option<VisualNarrationReport>,
}
