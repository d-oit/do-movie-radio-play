//! Content-addressed SFX cache and provenance records.
//!
//! Assets are stored once per content checksum under `objects/`. A small index
//! keyed by provider asset identity records where an asset came from and which
//! licence applies, so an offline run can reproduce a render without contacting
//! the provider.

use anyhow::{bail, Context, Result};
use movie_radio_types::{
    SfxAssetProvenance, SfxCandidate, SfxLicense, SfxQuery, SfxTransformation,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RECORD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheRecord {
    schema_version: u32,
    provenance: SfxAssetProvenance,
}

/// An asset served from the cache, with its bytes already checksum-verified.
#[derive(Debug, Clone)]
pub struct CachedSfx {
    pub provenance: SfxAssetProvenance,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheCleanup {
    pub removed_assets: usize,
    pub removed_records: usize,
    pub kept_assets: usize,
}

pub struct SfxCache {
    root: PathBuf,
}

impl SfxCache {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("objects"))
            .with_context(|| format!("cannot create SFX cache {}", root.display()))?;
        fs::create_dir_all(root.join("index"))
            .with_context(|| format!("cannot create SFX cache index {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Stores an asset and returns the provenance that describes it. Refuses
    /// empty or oversized assets rather than caching unusable bytes.
    pub fn put(
        &self,
        candidate: &SfxCandidate,
        bytes: &[u8],
        transformation: SfxTransformation,
        max_bytes: u64,
    ) -> Result<SfxAssetProvenance> {
        if bytes.is_empty() {
            bail!("refusing to cache empty SFX asset {}", candidate.id);
        }
        if bytes.len() as u64 > max_bytes {
            bail!(
                "SFX asset {} too large to cache: {} > {}",
                candidate.id,
                bytes.len(),
                max_bytes
            );
        }
        let checksum = checksum_bytes(bytes);
        let object = self.object_path(&checksum);
        if !object.exists() {
            write_atomic(&object, bytes)?;
        }
        let provenance = provenance_for(candidate, bytes, transformation);
        let record = CacheRecord {
            schema_version: RECORD_SCHEMA_VERSION,
            provenance: provenance.clone(),
        };
        write_atomic(
            &self.record_path(&candidate.provider, &candidate.id),
            &serde_json::to_vec_pretty(&record).context("serialize SFX cache record")?,
        )?;
        Ok(provenance)
    }

    /// Returns a cached asset only when the stored bytes still match its
    /// checksum; a corrupt entry is discarded so it cannot reach a render.
    pub fn get(&self, provider: &str, asset_id: &str) -> Option<CachedSfx> {
        let record_path = self.record_path(provider, asset_id);
        let record: CacheRecord = serde_json::from_slice(&fs::read(record_path).ok()?).ok()?;
        if record.schema_version != RECORD_SCHEMA_VERSION {
            return None;
        }
        let provenance = record.provenance;
        let bytes = fs::read(self.object_path(&provenance.checksum)).ok()?;
        if checksum_bytes(&bytes) != provenance.checksum {
            tracing::warn!(
                provider = %provenance.provider,
                asset = %provenance.provider_asset_id,
                "discarding corrupt SFX cache entry"
            );
            let _ = fs::remove_file(self.record_path(provider, asset_id));
            return None;
        }
        Some(CachedSfx { provenance, bytes })
    }

    /// Cached assets matching a query, sorted deterministically by provider and
    /// asset ID so offline selections are reproducible.
    pub fn search(&self, query: &SfxQuery) -> Vec<CachedSfx> {
        let mut matches: Vec<CachedSfx> = self
            .provenance_records()
            .into_iter()
            .filter(|p| matches_query(p, query))
            .filter_map(|p| self.get(&p.provider, &p.provider_asset_id))
            .collect();
        matches.sort_by(|a, b| {
            a.provenance
                .provider
                .cmp(&b.provenance.provider)
                .then_with(|| {
                    a.provenance
                        .provider_asset_id
                        .cmp(&b.provenance.provider_asset_id)
                })
        });
        matches
    }

    pub fn provenance_records(&self) -> Vec<SfxAssetProvenance> {
        let index = self.root.join("index");
        let Ok(entries) = fs::read_dir(index) else {
            return Vec::new();
        };
        let mut records = Vec::new();
        for entry in entries.flatten() {
            let Ok(bytes) = fs::read(entry.path()) else {
                continue;
            };
            let Ok(record) = serde_json::from_slice::<CacheRecord>(&bytes) else {
                continue;
            };
            if record.schema_version == RECORD_SCHEMA_VERSION {
                records.push(record.provenance);
            }
        }
        records.sort_by(|a, b| {
            a.provider
                .cmp(&b.provider)
                .then_with(|| a.provider_asset_id.cmp(&b.provider_asset_id))
        });
        records
    }

    /// Removes records and objects that no retained manifest references. Assets
    /// named in `protected_checksums` are always kept.
    pub fn cleanup(
        &self,
        protected_checksums: &BTreeSet<String>,
        dry_run: bool,
    ) -> Result<CacheCleanup> {
        let mut report = CacheCleanup::default();
        for record in self.provenance_records() {
            if protected_checksums.contains(&record.checksum) {
                report.kept_assets += 1;
                continue;
            }
            if dry_run
                || fs::remove_file(self.record_path(&record.provider, &record.provider_asset_id))
                    .is_ok()
            {
                report.removed_records += 1;
            }
        }

        let objects = self.root.join("objects");
        if let Ok(entries) = fs::read_dir(&objects) {
            for entry in entries.flatten() {
                let path = entry.path();
                let checksum = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                if protected_checksums.contains(&checksum) {
                    report.kept_assets += 1;
                    continue;
                }
                if dry_run || fs::remove_file(&path).is_ok() {
                    report.removed_assets += 1;
                }
            }
        }
        Ok(report)
    }

    fn object_path(&self, checksum: &str) -> PathBuf {
        self.root.join("objects").join(format!("{checksum}.asset"))
    }

    fn record_path(&self, provider: &str, asset_id: &str) -> PathBuf {
        let key = checksum_text(&format!("{provider}\u{0}{asset_id}"));
        self.root.join("index").join(format!("{key}.json"))
    }
}

fn matches_query(provenance: &SfxAssetProvenance, query: &SfxQuery) -> bool {
    let mut terms: Vec<String> = query
        .tags
        .iter()
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if let Some(mood) = query.mood.as_ref().filter(|m| !m.is_empty()) {
        terms.push(mood.to_lowercase());
    }
    if let Some(prompt) = query.prompt.as_ref().filter(|p| !p.is_empty()) {
        terms.extend(prompt.split_whitespace().map(str::to_lowercase));
    }
    if terms.is_empty() {
        return true;
    }
    let id = provenance.provider_asset_id.to_lowercase();
    terms.iter().any(|term| {
        provenance
            .tags
            .iter()
            .any(|tag| tag.to_lowercase() == *term)
            || id == *term
            || provenance.source_url.to_lowercase().contains(term)
    })
}

/// Builds the provenance record for fetched bytes. Used both by the cache and
/// by runs that have caching disabled.
pub fn provenance_for(
    candidate: &SfxCandidate,
    bytes: &[u8],
    transformation: SfxTransformation,
) -> SfxAssetProvenance {
    SfxAssetProvenance {
        provider: candidate.provider.clone(),
        provider_asset_id: candidate.id.clone(),
        source_url: candidate.path_or_url.clone(),
        license: candidate.license.clone(),
        attribution: candidate
            .attribution
            .clone()
            .or_else(|| fallback_attribution(candidate)),
        retrieved_unix_ms: now_unix_ms(),
        checksum: checksum_bytes(bytes),
        source_sample_rate_hz: detect_sample_rate(bytes),
        tags: candidate.tags.clone(),
        transformation,
    }
}

fn fallback_attribution(candidate: &SfxCandidate) -> Option<String> {
    if license_requires_attribution(&candidate.license) {
        Some(format!("SFX “{}” by {}", candidate.id, candidate.provider))
    } else {
        None
    }
}

fn license_requires_attribution(license: &SfxLicense) -> bool {
    matches!(
        license,
        SfxLicense::CcBy
            | SfxLicense::CcByNc
            | SfxLicense::CcBySa
            | SfxLicense::CcByNcSa
            | SfxLicense::SamplingPlus
    )
}

fn detect_sample_rate(bytes: &[u8]) -> u32 {
    hound::WavReader::new(Cursor::new(bytes))
        .map(|reader| reader.spec().sample_rate)
        .unwrap_or(0)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// FNV-1a 64-bit, matching the digest used for production artifacts.
pub fn checksum_bytes(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn checksum_text(text: &str) -> String {
    checksum_bytes(text.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let tmp = path.with_extension("part");
    let mut file =
        fs::File::create(&tmp).with_context(|| format!("cannot write {}", tmp.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    file.sync_all().ok();
    fs::rename(&tmp, path).with_context(|| format!("cannot commit {}", path.display()))?;
    Ok(())
}

/// Reads a checksum value without keeping a stale file handle around.
pub fn checksum_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok(checksum_bytes(&buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, provider: &str, tags: &[&str]) -> SfxCandidate {
        SfxCandidate {
            id: id.to_string(),
            path_or_url: format!("https://example.test/{id}.wav"),
            license: SfxLicense::CcBy,
            duration_secs: None,
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            provider: provider.to_string(),
            attribution: None,
        }
    }

    #[test]
    fn put_then_get_verifies_checksum() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = SfxCache::new(dir.path())?;
        let provenance = cache.put(
            &candidate("rain", "freesound", &["rain"]),
            b"asset-bytes",
            SfxTransformation::default(),
            1024,
        )?;
        assert_eq!(provenance.checksum, checksum_bytes(b"asset-bytes"));
        assert!(provenance.attribution.is_some());
        let cached = cache.get("freesound", "rain").expect("cached");
        assert_eq!(cached.bytes, b"asset-bytes");
        Ok(())
    }

    #[test]
    fn corrupt_object_is_rejected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = SfxCache::new(dir.path())?;
        let provenance = cache.put(
            &candidate("rain", "freesound", &["rain"]),
            b"asset-bytes",
            SfxTransformation::default(),
            1024,
        )?;
        fs::write(cache.object_path(&provenance.checksum), b"tampered")?;
        assert!(cache.get("freesound", "rain").is_none());
        assert!(cache.get("freesound", "rain").is_none());
        Ok(())
    }

    #[test]
    fn oversized_and_empty_assets_are_rejected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = SfxCache::new(dir.path())?;
        let cand = candidate("rain", "freesound", &["rain"]);
        assert!(cache
            .put(&cand, b"", SfxTransformation::default(), 1024)
            .is_err());
        assert!(cache
            .put(&cand, &vec![0u8; 2048], SfxTransformation::default(), 1024)
            .is_err());
        Ok(())
    }

    #[test]
    fn cleanup_keeps_protected_assets() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = SfxCache::new(dir.path())?;
        let keep = cache.put(
            &candidate("keep", "freesound", &["rain"]),
            b"keep-bytes",
            SfxTransformation::default(),
            1024,
        )?;
        cache.put(
            &candidate("drop", "freesound", &["rain"]),
            b"drop-bytes",
            SfxTransformation::default(),
            1024,
        )?;
        let protected = BTreeSet::from([keep.checksum.clone()]);
        let report = cache.cleanup(&protected, false)?;
        assert_eq!(report.removed_assets, 1);
        assert!(cache.get("freesound", "keep").is_some());
        Ok(())
    }
}
