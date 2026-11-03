//! Quality-gate, warning and usage summaries for a run manifest.

use serde::Deserialize;
use std::path::Path;

use super::build::RunMetadata;
use super::{
    read_json_optional, ArtifactChecksum, GateStatus, NarrationLink, QualityGate, RunStatus,
    StageRunStatus, StageStatus, UsageEstimate,
};
use movie_radio_render::mastering::{CheckStatus, MasteringReport};
use movie_radio_verification::VerificationReport;

pub(super) fn quality_gates(
    run_dir: &Path,
    artifacts: &[ArtifactChecksum],
    status: RunStatus,
) -> Vec<QualityGate> {
    let mut gates = Vec::new();
    gates.push(
        match read_json_optional::<VerificationReport>(&run_dir.join("verification.json")) {
            Some(report) => {
                let summary = &report.summary;
                let gate_status = if summary.rejected_count > 0 {
                    GateStatus::Fail
                } else if summary.suspicious_count > 0 {
                    GateStatus::Warn
                } else {
                    GateStatus::Pass
                };
                QualityGate {
                    name: "verification".to_string(),
                    status: gate_status,
                    detail: format!(
                        "{} segments: {} verified, {} suspicious, {} rejected",
                        summary.total_segments,
                        summary.verified_count,
                        summary.suspicious_count,
                        summary.rejected_count
                    ),
                }
            }
            None => QualityGate {
                name: "verification".to_string(),
                status: GateStatus::Skipped,
                detail: "verification.json not present".to_string(),
            },
        },
    );
    gates.push(
        match read_json_optional::<MasteringReport>(&run_dir.join("mastering-report.json")) {
            Some(report) => QualityGate {
                name: "mastering".to_string(),
                status: match report.overall {
                    CheckStatus::Pass => GateStatus::Pass,
                    CheckStatus::Warn => GateStatus::Warn,
                    CheckStatus::Fail => GateStatus::Fail,
                },
                detail: format!("profile {:?}: {}", report.profile, report.failure_summary()),
            },
            None => QualityGate {
                name: "mastering".to_string(),
                status: GateStatus::Skipped,
                detail: "mastering-report.json not present".to_string(),
            },
        },
    );
    let missing: Vec<&str> = artifacts
        .iter()
        .filter(|artifact| !artifact.present)
        .map(|artifact| artifact.path.as_str())
        .collect();
    gates.push(QualityGate {
        name: "artifacts".to_string(),
        status: if missing.is_empty() {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        detail: if missing.is_empty() {
            format!("{} artifacts present", artifacts.len())
        } else {
            format!("missing or empty: {}", missing.join(", "))
        },
    });
    gates.push(QualityGate {
        name: "manifest".to_string(),
        status: if status == RunStatus::Failed {
            GateStatus::Fail
        } else {
            GateStatus::Pass
        },
        detail: format!("run status {status:?}"),
    });
    gates
}

pub(super) fn warnings(
    stages: &[StageStatus],
    narration: &[NarrationLink],
    status: RunStatus,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for stage in stages {
        if stage.status == StageRunStatus::Failed {
            warnings.push(format!(
                "stage '{}' failed: {}",
                stage.name,
                stage.error.as_deref().unwrap_or("unknown error")
            ));
        }
    }
    let failed_narration = narration
        .iter()
        .filter(|link| link.output_start_ms.is_none())
        .count();
    if failed_narration > 0 {
        warnings.push(format!(
            "{failed_narration} narration item(s) have no synthesized output"
        ));
    }
    if status == RunStatus::Incomplete {
        warnings.push("run is incomplete: a required stage or artifact is missing".to_string());
    }
    warnings
}

pub(super) fn usage(
    run_dir: &Path,
    metadata: &RunMetadata,
    sfx_assets_used: usize,
) -> UsageEstimate {
    let entries = read_json_optional::<NarrationIndex>(&run_dir.join("narration.json"))
        .map(|index| index.entries)
        .unwrap_or_default();
    let succeeded = entries.iter().filter(|entry| entry.file.is_some()).count() as u64;
    let requests = entries.len() as u64;
    UsageEstimate {
        narration_requests: requests,
        narration_succeeded: succeeded,
        narration_failed: requests.saturating_sub(succeeded),
        sfx_assets_used: sfx_assets_used as u64,
        total_cost_usd: None,
        budget_usd: if metadata.max_cost_per_run_usd > 0.0 {
            Some(metadata.max_cost_per_run_usd)
        } else {
            None
        },
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct NarrationIndex {
    pub(super) entries: Vec<NarrationEntry>,
}

#[derive(Debug, Deserialize)]
pub(super) struct NarrationEntry {
    pub(super) file: Option<String>,
    pub(super) provider: Option<String>,
}
