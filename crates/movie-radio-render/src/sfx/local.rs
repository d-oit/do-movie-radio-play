use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use movie_radio_types::{
    LocalSfxConfig, SfxCandidate, SfxLicense, SfxProviderCapabilities, SfxQuery,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::SoundEffectBackend;

const SUPPORTED_EXTS: &[&str] = &["wav", "mp3", "flac", "ogg"];

#[derive(Debug, Clone)]
struct IndexedFile {
    path: PathBuf,
    tags: Vec<String>,
    license: SfxLicense,
}

pub struct LocalSfxBackend {
    config: LocalSfxConfig,
    root: PathBuf,
    index: Vec<IndexedFile>,
}

impl LocalSfxBackend {
    pub fn new(config: LocalSfxConfig) -> Result<Self> {
        let root = if config.root.is_empty() {
            PathBuf::from("./assets/sfx")
        } else {
            PathBuf::from(&config.root)
        };
        let mut backend = Self {
            config,
            root: root.clone(),
            index: Vec::new(),
        };
        if root.exists() {
            backend.rebuild_index()?;
        }
        Ok(backend)
    }

    pub fn with_root_for_test(root: PathBuf, config: LocalSfxConfig) -> Self {
        Self {
            config,
            root,
            index: Vec::new(),
        }
    }

    pub fn rebuild_index(&mut self) -> Result<()> {
        self.index.clear();
        if !self.root.exists() {
            return Ok(());
        }
        let canonical_root = self
            .root
            .canonicalize()
            .with_context(|| format!("failed to canonicalize sfx root {}", self.root.display()))?;
        self.scan_dir(&canonical_root, &canonical_root)?;
        self.index.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(())
    }

    fn scan_dir(&mut self, dir: &Path, canonical_root: &Path) -> Result<()> {
        let entries =
            std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() {
                if self.config.recursive {
                    self.scan_dir(&path, canonical_root)?;
                }
                continue;
            }
            if ft.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if SUPPORTED_EXTS.contains(&ext.to_lowercase().as_str()) {
                        let tags = tags_from_path(&path, canonical_root);
                        self.index.push(IndexedFile {
                            path,
                            tags,
                            license: SfxLicense::Cc0,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_fetch_path(&self, candidate: &SfxCandidate) -> Result<PathBuf> {
        let candidate_path = PathBuf::from(&candidate.path_or_url);
        if candidate_path.is_absolute() {
            let canonical = candidate_path
                .canonicalize()
                .with_context(|| format!("canonicalize {}", candidate_path.display()))?;
            let canonical_root = self
                .root
                .canonicalize()
                .with_context(|| format!("canonicalize root {}", self.root.display()))?;
            if !canonical.starts_with(&canonical_root) {
                bail!("path traversal rejected: {}", candidate.path_or_url);
            }
            Ok(canonical)
        } else {
            let joined = self.root.join(&candidate_path);
            let canonical = joined
                .canonicalize()
                .with_context(|| format!("canonicalize {}", joined.display()))?;
            let canonical_root = self
                .root
                .canonicalize()
                .with_context(|| format!("canonicalize root {}", self.root.display()))?;
            if !canonical.starts_with(&canonical_root) {
                bail!("path traversal rejected: {}", candidate.path_or_url);
            }
            Ok(canonical)
        }
    }

    fn score_query(&self, query: &SfxQuery, file: &IndexedFile) -> i32 {
        let mut score = 0;
        let query_tags: HashMap<String, ()> =
            query.tags.iter().map(|t| (t.to_lowercase(), ())).collect();
        for tag in &file.tags {
            if query_tags.contains_key(&tag.to_lowercase()) {
                score += 10;
            }
        }
        if let Some(mood) = &query.mood {
            for tag in &file.tags {
                if tag.to_lowercase() == mood.to_lowercase() {
                    score += 5;
                }
            }
        }
        if let Some(prompt) = &query.prompt {
            let needle = prompt.to_lowercase();
            for tag in &file.tags {
                if tag.to_lowercase().contains(&needle) || needle.contains(&tag.to_lowercase()) {
                    score += 3;
                }
            }
        }
        score
    }

    #[cfg(test)]
    pub fn index_len(&self) -> usize {
        self.index.len()
    }
}

fn tags_from_path(path: &Path, root: &Path) -> Vec<String> {
    let mut tags = Vec::new();
    if let Ok(rel) = path.strip_prefix(root) {
        for comp in rel.components() {
            if let std::path::Component::Normal(os) = comp {
                if let Some(s) = os.to_str() {
                    tags.push(s.to_lowercase());
                    if let Some(stem) = Path::new(s).file_stem().and_then(|x| x.to_str()) {
                        for part in stem.split(['_', '-', ' ']) {
                            let p = part.to_lowercase();
                            if !p.is_empty() && p != stem.to_lowercase() {
                                tags.push(p);
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        tags.push(stem.to_lowercase());
    }
    tags.sort();
    tags.dedup();
    tags
}

#[async_trait]
impl SoundEffectBackend for LocalSfxBackend {
    async fn search(&self, query: &SfxQuery) -> Result<Vec<SfxCandidate>> {
        let mut scored: Vec<(i32, &IndexedFile)> = self
            .index
            .iter()
            .map(|f| (self.score_query(query, f), f))
            .filter(|(s, _)| *s > 0 || query.tags.is_empty())
            .collect();
        scored.sort_by(|(sa, fa), (sb, fb)| sb.cmp(sa).then_with(|| fa.path.cmp(&fb.path)));
        let candidates = scored
            .into_iter()
            .map(|(score, f)| {
                let _ = score;
                SfxCandidate {
                    id: f
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("sfx")
                        .to_string(),
                    path_or_url: f.path.display().to_string(),
                    license: f.license.clone(),
                    duration_secs: None,
                    tags: f.tags.clone(),
                    provider: "local".to_string(),
                }
            })
            .collect();
        Ok(candidates)
    }

    async fn fetch(&self, candidate: &SfxCandidate) -> Result<Vec<u8>> {
        let path = self.validate_fetch_path(candidate)?;
        let meta =
            std::fs::metadata(&path).with_context(|| format!("metadata {}", path.display()))?;
        if meta.len() > self.config.max_file_bytes {
            bail!(
                "file too large: {} bytes > {} limit",
                meta.len(),
                self.config.max_file_bytes
            );
        }
        let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        Ok(bytes)
    }

    fn capabilities(&self) -> SfxProviderCapabilities {
        SfxProviderCapabilities {
            supports_search: true,
            supports_fetch: true,
            supports_generate: false,
            requires_network: false,
            is_paid: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_wav_bytes() -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        let mut writer = hound::WavWriter::new(&mut cursor, spec).expect("writer");
        for i in 0..160 {
            let s = (i as f32 * 0.1).sin() * 1000.0;
            writer.write_sample(s as i16).expect("sample");
        }
        writer.finalize().expect("finalize");
        buf
    }

    #[tokio::test]
    async fn test_local_index_and_search() -> Result<()> {
        let dir = tempdir()?;
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("ambience"))?;
        std::fs::write(
            root.join("ambience").join("rain_soft.wav"),
            make_wav_bytes(),
        )?;
        std::fs::write(root.join("forest_birds.wav"), make_wav_bytes())?;
        let mut backend = LocalSfxBackend::with_root_for_test(
            root.clone(),
            LocalSfxConfig {
                root: root.display().to_string(),
                recursive: true,
                max_file_bytes: 50_000_000,
            },
        );
        backend.rebuild_index()?;
        assert_eq!(backend.index_len(), 2);
        let q = SfxQuery {
            tags: vec!["rain".to_string()],
            mood: None,
            duration_secs: None,
            prompt: None,
        };
        let results = backend.search(&q).await?;
        assert!(!results.is_empty());
        assert!(results[0].tags.iter().any(|t| t.contains("rain")));
        Ok(())
    }

    #[tokio::test]
    async fn test_path_traversal_rejected() -> Result<()> {
        let dir = tempdir()?;
        let root = dir.path().to_path_buf();
        std::fs::write(root.join("safe.wav"), make_wav_bytes())?;
        let mut backend = LocalSfxBackend::with_root_for_test(
            root.clone(),
            LocalSfxConfig {
                root: root.display().to_string(),
                recursive: false,
                max_file_bytes: 50_000_000,
            },
        );
        backend.rebuild_index()?;
        let candidate = SfxCandidate {
            id: "evil".to_string(),
            path_or_url: "../etc/passwd".to_string(),
            license: SfxLicense::Cc0,
            duration_secs: None,
            tags: Vec::new(),
            provider: "local".to_string(),
        };
        let res = backend.fetch(&candidate).await;
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("traversal") || msg.contains("canonicalize"),
            "msg={msg}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_deterministic_sort() -> Result<()> {
        let dir = tempdir()?;
        let root = dir.path().to_path_buf();
        for name in ["b.wav", "a.wav", "c.wav"] {
            std::fs::write(root.join(name), make_wav_bytes())?;
        }
        let mut backend = LocalSfxBackend::with_root_for_test(
            root.clone(),
            LocalSfxConfig {
                root: root.display().to_string(),
                recursive: false,
                max_file_bytes: 50_000_000,
            },
        );
        backend.rebuild_index()?;
        let q = SfxQuery::default();
        let r1 = backend.search(&q).await?;
        let r2 = backend.search(&q).await?;
        assert_eq!(
            r1.iter().map(|c| &c.path_or_url).collect::<Vec<_>>(),
            r2.iter().map(|c| &c.path_or_url).collect::<Vec<_>>()
        );
        Ok(())
    }
}
