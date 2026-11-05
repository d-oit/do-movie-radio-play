use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::assemble::{RadioPlayAssembler, SfxSegment};
use crate::{Action, PipelineContext, WorldState};
use movie_radio_pipeline::pipeline::sfx_autofill::autofill_silent_scene_sfx;
use movie_radio_render::sfx::SfxManager;
use movie_radio_types::SfxTrigger;

#[derive(Debug, Default)]
pub struct AssembleRadioPlay;

#[async_trait]
impl Action for AssembleRadioPlay {
    fn name(&self) -> &str {
        "assemble_radio_play"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            narrator_voice_synthesized: true,
            movie_decoded: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        1.5
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let original = ctx
            .original_audio
            .as_ref()
            .context("Original audio not decoded")?;
        let scripts = ctx.scripts.as_ref().context("Scripts not generated")?;

        let assembler = RadioPlayAssembler::new(ctx.sample_rate, 50, 0.3)
            .with_time_stretch(true, 0)
            .with_dialogue_safety(250, 1.25);
        let narration_segments = assembler.build_narration_segments(scripts, &ctx.narration_audio);

        let mut sfx_segments = Vec::new();
        let mut sfx_assets = Vec::new();
        if let Some(ref mut timeline) = ctx.timeline {
            autofill_silent_scene_sfx(timeline);
            let sfx_cfg = ctx.config.sound_effects.clone().unwrap_or_default();
            if let Ok(sfx_mgr) = SfxManager::from_config(&sfx_cfg) {
                for seg in &timeline.segments {
                    if let Some(ref trigger) = seg.sfx_trigger {
                        if *trigger != SfxTrigger::None {
                            let duration_secs = ((seg.end_ms.saturating_sub(seg.start_ms)) as f32
                                / 1000.0)
                                .clamp(0.0, 300.0);
                            let rendered = sfx_mgr
                                .render_trigger(trigger, ctx.sample_rate, Some(duration_secs))
                                .await?;
                            if let Some(rendered) = rendered {
                                let start_sample = (seg.start_ms as f64 * ctx.sample_rate as f64
                                    / 1000.0)
                                    as usize;
                                sfx_segments.push(SfxSegment {
                                    start_sample,
                                    samples: rendered.samples,
                                    provenance: Some(rendered.provenance.clone()),
                                });
                                sfx_assets.push(rendered.provenance);
                            }
                        }
                    }
                }
            }
        }

        let radio_play =
            assembler.assemble_with_sfx(original, &narration_segments, &sfx_segments)?;
        let track_map =
            assembler.build_track_map(original.len(), &narration_segments, &sfx_segments);
        ctx.track_map = Some(track_map);
        ctx.sfx_assets = sfx_assets;

        // Master the deliverable to the configured delivery profile. The
        // pre-mastering mix stays in `assembled_audio` so preservation checks
        // compare like with like.
        let is_mp3 = ctx
            .output_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("mp3"));
        let mut profile =
            movie_radio_render::mastering::MasteringProfile::from_name(&ctx.mastering_profile)
                .unwrap_or_default();
        if is_mp3 {
            // Lossy encoding can overshoot the sample-domain true peak; leave
            // an extra half decibel so the encoded file still meets the ceiling.
            profile.max_true_peak_dbtp = profile.max_true_peak_dbtp.min(-1.5);
        }
        if let Some(parent) = ctx.output_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Stream the mastered deliverable to disk instead of holding a second
        // full-length buffer next to the assembled mix. Both the mastering
        // writer and the MP3 encoder write incrementally, so they write to a
        // staging file that is only moved into place once complete.
        let staged_wav = StagedDeliverable::new(&ctx.output_path)?;
        let mastered = movie_radio_render::mastering::master_to_wav(
            &radio_play,
            staged_wav.path(),
            ctx.sample_rate,
            1,
            &profile,
        )?;
        ctx.mastering_report = Some(mastered.report);

        if is_mp3 {
            let staged_mp3 = StagedDeliverable::new(&ctx.output_path)?;
            encode_to_mp3(staged_wav.path(), staged_mp3.path())?;
            staged_mp3.commit()?;
        } else {
            staged_wav.commit()?;
        }

        info!(
            output = %ctx.output_path.display(),
            duration_s = radio_play.len() as f64 / ctx.sample_rate as f64,
            "Radio play assembled"
        );
        ctx.assembled_audio = Some(radio_play);
        Ok(())
    }
}

fn encode_to_mp3(wav_path: &std::path::Path, mp3_path: &std::path::Path) -> Result<()> {
    use std::process::Command;

    let status = Command::new("ffmpeg")
        .arg("-nostdin")
        .arg("-protocol_whitelist")
        .arg("file,pipe,fd")
        .args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(wav_path)
        .args(["-codec:a", "libmp3lame", "-b:a", "192k", "-q:a", "2", "-y"])
        .arg(mp3_path)
        .status()?;

    if !status.success() {
        bail!("ffmpeg MP3 encoding failed with exit code: {}", status);
    }
    Ok(())
}

/// A deliverable written through a staging file in its destination directory.
///
/// Mastering and MP3 encoding both write incrementally, so a failure or an
/// interrupted run would otherwise leave a truncated file at the final path
/// that looks like a valid deliverable. Staging keeps the final path untouched
/// until the bytes are complete, and the staging file is removed on drop when
/// the run never commits.
struct StagedDeliverable {
    temp: tempfile::NamedTempFile,
    final_path: PathBuf,
}

impl StagedDeliverable {
    fn new(final_path: &Path) -> Result<Self> {
        // Staging in the destination directory keeps the commit a same-filesystem
        // rename, which is atomic.
        let dir = final_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        // Keep the destination's extension: ffmpeg infers the container from
        // it, and a bare temp name would make the MP3 muxer ambiguous.
        let suffix = final_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!(".{ext}"));
        let mut builder = tempfile::Builder::new();
        if let Some(ref suffix) = suffix {
            builder.suffix(suffix);
        }
        let temp = builder
            .tempfile_in(dir)
            .with_context(|| format!("cannot create staging file in {}", dir.display()))?;
        Ok(Self {
            temp,
            final_path: final_path.to_path_buf(),
        })
    }

    fn path(&self) -> &Path {
        self.temp.path()
    }

    fn commit(self) -> Result<()> {
        let final_path = self.final_path.clone();
        self.temp
            .persist(&final_path)
            .map_err(|error| error.error)
            .with_context(|| {
                format!(
                    "cannot move staged deliverable into {}",
                    final_path.display()
                )
            })?;
        // `NamedTempFile` creates 0600 files; a deliverable should be readable
        // like any other output.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&final_path, std::fs::Permissions::from_mode(0o644))
                .with_context(|| format!("cannot set permissions on {}", final_path.display()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn stray_entries(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn commit_moves_staged_bytes_into_place() {
        let dir = tempfile::tempdir().expect("tempdir");
        let final_path = dir.path().join("deliverable.wav");

        let staged = StagedDeliverable::new(&final_path).expect("stage");
        assert!(
            !final_path.exists(),
            "final path must stay absent while staged"
        );
        std::fs::write(staged.path(), b"complete").expect("write staged");
        staged.commit().expect("commit");

        assert_eq!(std::fs::read(&final_path).expect("read"), b"complete");
        assert_eq!(
            stray_entries(dir.path()),
            vec!["deliverable.wav".to_string()],
            "staging file must not survive a commit"
        );
    }

    #[test]
    fn dropping_without_commit_leaves_no_partial_deliverable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let final_path = dir.path().join("deliverable.wav");

        {
            let mut staged = StagedDeliverable::new(&final_path).expect("stage");
            staged.temp.write_all(b"truncated").expect("write staged");
            // Simulates a failure or interruption before commit.
        }

        assert!(
            !final_path.exists(),
            "an uncommitted run must not leave a deliverable"
        );
        assert!(
            stray_entries(dir.path()).is_empty(),
            "staging file must be cleaned up: {:?}",
            stray_entries(dir.path())
        );
    }

    #[test]
    fn commit_replaces_an_existing_deliverable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let final_path = dir.path().join("deliverable.wav");
        std::fs::write(&final_path, b"previous").expect("write previous");

        let staged = StagedDeliverable::new(&final_path).expect("stage");
        std::fs::write(staged.path(), b"replacement").expect("write staged");
        staged.commit().expect("commit");

        assert_eq!(std::fs::read(&final_path).expect("read"), b"replacement");
        assert_eq!(stray_entries(dir.path()).len(), 1);
    }

    #[test]
    fn staged_mp3_keeps_its_extension_for_ffmpeg() {
        if std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_err()
        {
            eprintln!("ffmpeg unavailable; skipping MP3 staging check");
            return;
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let wav_path = dir.path().join("source.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav_path, spec).expect("create wav");
        for i in 0..16_000 {
            let t = i as f32 / 16_000.0;
            let sample = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.2;
            writer
                .write_sample((sample * i16::MAX as f32) as i16)
                .expect("write sample");
        }
        writer.finalize().expect("finalize wav");

        let final_path = dir.path().join("deliverable.mp3");
        let staged = StagedDeliverable::new(&final_path).expect("stage");
        assert!(
            staged.path().extension().and_then(|ext| ext.to_str()) == Some("mp3"),
            "staging file must keep the .mp3 suffix, got {:?}",
            staged.path()
        );
        encode_to_mp3(&wav_path, staged.path()).expect("encode mp3");
        staged.commit().expect("commit");

        let bytes = std::fs::read(&final_path).expect("read mp3");
        assert!(!bytes.is_empty(), "encoded MP3 must not be empty");
        assert!(
            bytes.starts_with(b"ID3") || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0),
            "output does not look like an MP3 stream"
        );
    }
}
