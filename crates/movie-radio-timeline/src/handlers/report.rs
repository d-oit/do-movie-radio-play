//! `report`: inspect, validate and re-generate a production run manifest.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

use movie_radio_goap::production::manifest::MANIFEST_FILE;
use movie_radio_goap::production::{self, RunManifest};

#[derive(Debug, Default, Clone)]
pub struct ReportOptions {
    pub run_dir: PathBuf,
    /// Print the manifest JSON to stdout.
    pub json: bool,
    /// Validate only; do not rewrite the manifest or report.
    pub validate: bool,
}

pub fn handle_report(opts: ReportOptions) -> Result<()> {
    if !opts.run_dir.is_dir() {
        bail!("not a production run directory: {}", opts.run_dir.display());
    }
    let manifest = production::build_manifest(&opts.run_dir)?;
    manifest.validate()?;

    if opts.validate {
        validate_on_disk(&opts.run_dir, &manifest)?;
        println!(
            "manifest valid: status={:?} stages={} artifacts={} warnings={}",
            manifest.status,
            manifest.stages.len(),
            manifest.artifacts.len(),
            manifest.warnings.len()
        );
        return Ok(());
    }

    let (manifest_path, report_path) =
        production::write_manifest_and_report(&opts.run_dir, &manifest)?;
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        eprintln!("manifest: {}", manifest_path.display());
        eprintln!("report:   {}", report_path.display());
        return Ok(());
    }
    println!(
        "run {}: {:?} ({} stages, {} artifacts, {} warnings)",
        manifest.run_id,
        manifest.status,
        manifest.stages.len(),
        manifest.artifacts.len(),
        manifest.warnings.len()
    );
    for stage in &manifest.stages {
        println!("  {:<20} {:?}", stage.name, stage.status);
    }
    println!("manifest: {}", manifest_path.display());
    println!("report:   {}", report_path.display());
    Ok(())
}

/// A previously written manifest must still parse and agree with the rebuilt one.
fn validate_on_disk(run_dir: &std::path::Path, rebuilt: &RunManifest) -> Result<()> {
    let path = run_dir.join(MANIFEST_FILE);
    if !path.exists() {
        return Ok(());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let on_disk: RunManifest =
        serde_json::from_str(&raw).with_context(|| format!("cannot parse {}", path.display()))?;
    on_disk.validate()?;
    if on_disk.schema_version != rebuilt.schema_version {
        bail!(
            "on-disk manifest schema {} disagrees with rebuilt schema {}",
            on_disk.schema_version,
            rebuilt.schema_version
        );
    }
    if on_disk.status != rebuilt.status {
        bail!(
            "on-disk manifest status {:?} disagrees with rebuilt status {:?}; re-run `timeline report {}`",
            on_disk.status,
            rebuilt.status,
            run_dir.display()
        );
    }
    Ok(())
}
