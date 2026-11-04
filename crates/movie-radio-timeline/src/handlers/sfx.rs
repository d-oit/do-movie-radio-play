//! `sfx cache`: inspect and garbage-collect the content-addressed SFX cache.

use anyhow::{bail, Context, Result};
use movie_radio_goap::production::{RunManifest, SFX_ASSETS_FILE};
use movie_radio_render::sfx::SfxCache;
use movie_radio_types::AppConfig;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone)]
pub struct SfxCacheInspectOptions {
    pub json: bool,
}

#[derive(Debug, Default, Clone)]
pub struct SfxCacheCleanOptions {
    /// Run directories whose manifests protect the assets they reference.
    pub run_dirs: Vec<PathBuf>,
    /// Allow cleanup when no manifest protects anything.
    pub all: bool,
    pub dry_run: bool,
}

pub fn handle_sfx_cache_inspect(cfg: &AppConfig, opts: SfxCacheInspectOptions) -> Result<()> {
    let cache = open_cache(cfg)?;
    let records = cache.provenance_records();
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&records)?);
        return Ok(());
    }
    println!(
        "SFX cache {}: {} asset(s)",
        cache.root().display(),
        records.len()
    );
    for record in records {
        println!(
            "  {}/{}  {}  {:?}  {}",
            record.provider,
            record.provider_asset_id,
            record.checksum,
            record.license,
            record.source_url
        );
    }
    Ok(())
}

pub fn handle_sfx_cache_clean(cfg: &AppConfig, opts: SfxCacheCleanOptions) -> Result<()> {
    if opts.run_dirs.is_empty() && !opts.all {
        bail!(
            "refusing to clean the whole SFX cache: pass --run-dir for each retained production \
             run, or --all to remove every cached asset"
        );
    }
    let protected = protected_checksums(&opts.run_dirs)?;
    let cache = open_cache(cfg)?;
    let report = cache.cleanup(&protected, opts.dry_run)?;
    let verb = if opts.dry_run {
        "would remove"
    } else {
        "removed"
    };
    println!(
        "{verb} {} asset(s) and {} index record(s); kept {} protected asset(s)",
        report.removed_assets, report.removed_records, report.kept_assets
    );
    Ok(())
}

fn open_cache(cfg: &AppConfig) -> Result<SfxCache> {
    let dir = &cfg.sound_effects.cache_dir;
    if dir.trim().is_empty() {
        bail!("sound_effects.cache_dir is empty; SFX caching is disabled");
    }
    SfxCache::new(dir)
}

fn protected_checksums(run_dirs: &[PathBuf]) -> Result<BTreeSet<String>> {
    let mut protected = BTreeSet::new();
    for run_dir in run_dirs {
        if !run_dir.is_dir() {
            bail!("not a production run directory: {}", run_dir.display());
        }
        collect_manifest_checksums(&run_dir.join("manifest.json"), &mut protected)?;
        collect_manifest_checksums(&run_dir.join(SFX_ASSETS_FILE), &mut protected)?;
    }
    Ok(protected)
}

fn collect_manifest_checksums(path: &Path, out: &mut BTreeSet<String>) -> Result<()> {
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(());
    };
    if let Ok(manifest) = serde_json::from_slice::<RunManifest>(&bytes) {
        out.extend(manifest.sfx_assets.into_iter().map(|a| a.checksum));
        return Ok(());
    }
    let assets: Vec<movie_radio_types::SfxAssetProvenance> = serde_json::from_slice(&bytes)
        .with_context(|| format!("cannot parse {}", path.display()))?;
    out.extend(assets.into_iter().map(|a| a.checksum));
    Ok(())
}
