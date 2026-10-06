//! Finds timed dialogue cues for a film, so narration can be vetoed against
//! real speech even when the user supplies no subtitles.
//!
//! Order: explicit `--subtitles`, sidecar `.srt`, embedded subtitle track,
//! speech-to-text (`scripts/transcribe_cues.py`). Each fallback is best-effort:
//! a failure logs why and the run continues on the detector alone.

use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

const TRANSCRIBE_SCRIPT: &str = "scripts/transcribe_cues.py";
const ENV_PYTHON: &str = "MRPLAY_PYTHON";

/// Sidecar candidates, most specific first: `film.<lang>.srt`, then `film.srt`.
pub(crate) fn sidecar_candidates(movie: &Path, language: Option<&str>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(lang) = language.filter(|l| !l.trim().is_empty()) {
        out.push(movie.with_extension(format!("{lang}.srt")));
    }
    out.push(movie.with_extension("srt"));
    out
}

/// True when the file holds at least one timed cue.
pub(crate) fn has_cues(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|s| s.contains("-->"))
}

fn extract_embedded(movie: &Path, out: &Path) -> bool {
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(movie)
        .args(["-map", "0:s:0", "-f", "srt"])
        .arg(out)
        .stderr(std::process::Stdio::null())
        .status();
    matches!(status, Ok(s) if s.success()) && has_cues(out)
}

/// The film's spoken language is unknown (`--language` is the narration
/// language), so the transcriber auto-detects it; only timings are used.
fn transcribe(movie: &Path, out: &Path) -> bool {
    let python = std::env::var(ENV_PYTHON).unwrap_or_else(|_| "python3".to_string());
    let mut cmd = Command::new(python);
    cmd.arg(TRANSCRIBE_SCRIPT).arg(movie).arg(out);
    match cmd.output() {
        Ok(o) if o.status.success() && has_cues(out) => true,
        Ok(o) => {
            let tail: String = String::from_utf8_lossy(&o.stderr)
                .lines()
                .last()
                .unwrap_or("")
                .to_string();
            warn!(%tail, "speech-to-text cue extraction failed (pip install faster-whisper)");
            false
        }
        Err(e) => {
            warn!(error = %e, "cannot run {TRANSCRIBE_SCRIPT}");
            false
        }
    }
}

/// Returns the cue file to use, or `None` when the film has none and none
/// could be derived. `cache` is where derived cues are written and reused.
pub(crate) fn resolve_cues(
    movie: &Path,
    explicit: Option<PathBuf>,
    language: Option<&str>,
    cache: &Path,
    auto: bool,
) -> Option<PathBuf> {
    if explicit.is_some() {
        return explicit;
    }
    if let Some(found) = sidecar_candidates(movie, language)
        .into_iter()
        .find(|p| has_cues(p))
    {
        info!(path = %found.display(), "using sidecar subtitles as dialogue cues");
        return Some(found);
    }
    if !auto {
        return None;
    }
    if has_cues(cache) {
        info!(path = %cache.display(), "reusing derived dialogue cues");
        return Some(cache.to_path_buf());
    }
    if extract_embedded(movie, cache) {
        info!("using embedded subtitle track as dialogue cues");
        return Some(cache.to_path_buf());
    }
    if transcribe(movie, cache) {
        info!(path = %cache.display(), "derived dialogue cues by speech-to-text");
        return Some(cache.to_path_buf());
    }
    warn!("no dialogue cues available: narration relies on the VAD alone and may overlap speech");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_prefers_language_specific_file() {
        let c = sidecar_candidates(Path::new("/m/film.mkv"), Some("de"));
        assert_eq!(
            c,
            vec![
                PathBuf::from("/m/film.de.srt"),
                PathBuf::from("/m/film.srt")
            ]
        );
        assert_eq!(
            sidecar_candidates(Path::new("/m/film.mkv"), None),
            vec![PathBuf::from("/m/film.srt")]
        );
    }

    #[test]
    fn explicit_wins_and_sidecar_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let movie = dir.path().join("film.mp4");
        std::fs::write(
            dir.path().join("film.srt"),
            "1\n00:00:01,000 --> 00:00:02,000\nx\n",
        )
        .unwrap();
        let cache = dir.path().join("cache.srt");
        let explicit = Some(PathBuf::from("/x.srt"));
        assert_eq!(
            resolve_cues(&movie, explicit.clone(), None, &cache, true),
            explicit
        );
        assert_eq!(
            resolve_cues(&movie, None, None, &cache, false),
            Some(dir.path().join("film.srt"))
        );
    }

    #[test]
    fn nothing_found_when_auto_disabled_and_cache_reused_when_valid() {
        let dir = tempfile::tempdir().unwrap();
        let movie = dir.path().join("film.mp4");
        let cache = dir.path().join("film.cues.srt");
        assert_eq!(resolve_cues(&movie, None, None, &cache, false), None);
        std::fs::write(&cache, "1\n00:00:01,000 --> 00:00:02,000\n[speech]\n").unwrap();
        assert_eq!(
            resolve_cues(&movie, None, None, &cache, true),
            Some(cache.clone())
        );
        assert!(!has_cues(&dir.path().join("missing.srt")));
    }
}
