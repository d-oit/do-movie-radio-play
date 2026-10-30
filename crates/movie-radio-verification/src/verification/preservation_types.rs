use movie_radio_types::SfxLicense;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackType {
    SourceFilm,
    Narration,
    Sfx,
    Background,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum GainDuckingOperation {
    None,
    Ducked { level: f32 },
    Gain { db: f32 },
    Crossfade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentProvenance {
    pub asset_id: String,
    pub source: String,
    pub license: SfxLicense,
    pub prompt: Option<String>,
}

impl Default for ContentProvenance {
    fn default() -> Self {
        Self {
            asset_id: "source_film".to_string(),
            source: "source_film".to_string(),
            license: SfxLicense::Cc0,
            prompt: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackMapEntry {
    pub id: String,
    pub track_type: TrackType,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub output_start_ms: u64,
    pub output_end_ms: u64,
    pub operation: GainDuckingOperation,
    pub provenance: ContentProvenance,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackMap {
    pub entries: Vec<TrackMapEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start_ms: u64,
    pub end_ms: u64,
}

impl TimeRange {
    pub fn new(start_ms: u64, end_ms: u64) -> Self {
        Self { start_ms, end_ms }
    }

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start_ms < other.end_ms && other.start_ms < self.end_ms
    }

    pub fn overlap_ms(&self, other: &TimeRange) -> u64 {
        if self.overlaps(other) {
            let start = self.start_ms.max(other.start_ms);
            let end = self.end_ms.min(other.end_ms);
            end.saturating_sub(start)
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservationConfig {
    /// Maximum RMS difference allowed between source and output in untouched regions
    pub max_untouched_rms_error: f32,
    /// Maximum allowed attenuation in dB for protected dialogue outside planned ducking
    pub max_dialogue_attenuation_db: f32,
    /// Target duck level expected during narration overlay
    pub duck_level: f32,
    /// Safety padding in ms around protected dialogue cues
    pub safety_padding_ms: u64,
    /// Maximum allowed overlap in ms between SFX and narration segments
    pub max_allowed_sfx_narration_overlap_ms: u64,
    /// Absolute sample peak threshold above which audio is flagged as clipping
    pub peak_clipping_threshold: f32,
    /// List of allowed license identifier strings
    pub allowed_licenses: Vec<String>,
}

impl Default for PreservationConfig {
    fn default() -> Self {
        Self {
            max_untouched_rms_error: 0.05,
            max_dialogue_attenuation_db: 1.0,
            duck_level: 0.2,
            safety_padding_ms: 50,
            max_allowed_sfx_narration_overlap_ms: 0,
            peak_clipping_threshold: 1.0,
            allowed_licenses: vec![
                "cc0".to_string(),
                "cc-by".to_string(),
                "public domain".to_string(),
                "mit".to_string(),
                "apache-2.0".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowType {
    UntouchedSource,
    ProtectedDialogue,
    NarrationPlacement,
    SfxPlacement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowVerificationVerdict {
    pub window_id: String,
    pub window_type: WindowType,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub output_start_ms: u64,
    pub output_end_ms: u64,
    pub passed: bool,
    pub failure_reasons: Vec<String>,
    pub measured_preservation_rms_diff: Option<f32>,
    pub measured_dialogue_attenuation_db: Option<f32>,
    pub measured_overlap_ms: Option<u64>,
    pub measured_peak_level: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseProvenanceVerdict {
    pub asset_id: String,
    pub license: String,
    pub allowed: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIntegrityResult {
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub codec_valid: bool,
    pub duration_matches_source: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservationSummary {
    pub total_windows: usize,
    pub passed_windows: usize,
    pub failed_windows: usize,
    pub total_assets: usize,
    pub verified_licenses: usize,
    pub failed_licenses: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservationReport {
    pub overall_passed: bool,
    pub track_map: TrackMap,
    pub window_verdicts: Vec<WindowVerificationVerdict>,
    pub license_provenance_summary: Vec<LicenseProvenanceVerdict>,
    pub file_integrity: FileIntegrityResult,
    pub summary: PreservationSummary,
}
