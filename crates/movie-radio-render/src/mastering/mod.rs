//! Broadcast-oriented mastering profiles, measurement and quality reports.
//!
//! A profile describes the delivery target (integrated loudness, true-peak
//! ceiling, loudness range and continuity limits). The same profile drives
//! normalization/limiting and the pass/warn/fail report, so a rendered file is
//! measured against the target it was mastered for.

mod fixtures;
mod io;
mod limiter;
mod measure;
mod profiles;
mod report;

#[cfg(test)]
mod tests;

pub use fixtures::generate_fixtures;
pub use io::{read_audio, read_wav, validate_file, write_html, write_json, write_wav};
pub use limiter::{master, MasteringOutcome};
pub use measure::{measure, LoudnessMeasurement, MeasurementConfig, SILENCE_LUFS};
pub use profiles::{MasteringProfile, DEFAULT_PROFILE};
pub use report::{validate, CheckStatus, MasteringReport, QualityCheck};
