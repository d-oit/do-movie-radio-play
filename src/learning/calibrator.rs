use anyhow::{Context, Result};
use serde::Serialize;
use std::{fs, path::Path};

use crate::io::json::read_json;
use crate::learning::corrections::CorrectionRecord;
use crate::learning::profiles;

#[derive(Debug, Serialize)]
struct CalibrationReport {
    version: u32,
    profile: String,
    records_seen: usize,
    speech_to_non_voice: usize,
    non_voice_to_speech: usize,
    recommended_energy_threshold_delta: f32,
}

pub fn run_calibration(corrections_dir: &Path, profile: &str) -> Result<()> {
    let mut files = vec![];
    for e in fs::read_dir(corrections_dir)
        .with_context(|| format!("failed reading {}", corrections_dir.display()))?
    {
        let p = e?.path();
        if p.extension().and_then(|x| x.to_str()) == Some("json") {
            files.push(p);
        }
    }

    let mut speech_to_non_voice = 0usize;
    let mut non_voice_to_speech = 0usize;
    let mut total = 0usize;

    for file in files {
        let records: Vec<CorrectionRecord> = read_json(&file)
            .with_context(|| format!("malformed corrections JSON: {}", file.display()))?;
        for r in records {
            total += 1;
            if r.original_kind == "speech" && r.corrected_kind == "non_voice" {
                speech_to_non_voice += 1;
            }
            if r.original_kind == "non_voice" && r.corrected_kind == "speech" {
                non_voice_to_speech += 1;
            }
        }
    }

    let base = profiles::profile(profile);
    let drift = (non_voice_to_speech as f32 - speech_to_non_voice as f32) * 0.0005;
    let report = CalibrationReport {
        version: 1,
        profile: base.name,
        records_seen: total,
        speech_to_non_voice,
        non_voice_to_speech,
        recommended_energy_threshold_delta: base.energy_threshold_delta + drift,
    };
    let out_dir = Path::new("analysis/learnings");
    fs::create_dir_all(out_dir)?;
    let out_path = out_dir.join("latest-calibration.json");
    fs::write(out_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
