use anyhow::{bail, Result};
use std::path::PathBuf;
use tracing::info;

use movie_radio_goap::assemble::{NarrationSegment, RadioPlayAssembler, SfxSegment};
use movie_radio_goap::gaps::GapIdentifier;
use movie_radio_goap::narrate::{NarrationGenerator, NarrationScript};
use movie_radio_io::json::{read_timeline, write_json_pretty};
use movie_radio_pipeline::pipeline::decode::decode_audio;
use movie_radio_pipeline::pipeline::extract_timeline;
use movie_radio_pipeline::pipeline::sfx_autofill::autofill_silent_scene_sfx;
use movie_radio_render::sfx::SfxManager;
use movie_radio_types::{AnalysisConfig, SfxTrigger, SoundEffectsConfig, TimelineOutput};
use movie_radio_voice::config::VoiceSynthesisConfig;
use movie_radio_voice::voice::SynthesisOrchestrator;
use movie_radio_voice::voice::SynthesisRequest;

pub fn handle_radio_play(
    movie: PathBuf,
    timeline_path: Option<PathBuf>,
    subtitles_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
    analyze_only: bool,
) -> Result<()> {
    if analyze_only {
        info!(movie = %movie.display(), "Running visual gap analysis");

        let timeline = if let Some(p) = timeline_path {
            read_timeline(&p)?
        } else {
            bail!("--timeline is required for --analyze-only in this version");
        };

        let srt_content = if let Some(p) = subtitles_path {
            Some(std::fs::read_to_string(p)?)
        } else {
            None
        };

        let identifier = GapIdentifier::default();
        let srt_ref: Option<&str> = srt_content.as_deref();
        // skipcq: RS-E1015 — DeepSource false positive: srt_ref is Option<&str>, not unit-type
        let gap_analysis = identifier.identify_gaps(&timeline, srt_ref)?; // skipcq: RS-E1015

        if let Some(out) = output_path {
            write_json_pretty(&out, &gap_analysis)?;
            info!(gaps = gap_analysis.gaps.len(), output = %out.display(), "Gap analysis complete");
        } else {
            println!("{}", serde_json::to_string_pretty(&gap_analysis)?);
        }
    } else {
        info!(movie = %movie.display(), "Running full radio-play pipeline");
        run_full_pipeline(movie, timeline_path, subtitles_path, output_path)?;
    }
    Ok(())
}

fn run_full_pipeline(
    movie: PathBuf,
    timeline_path: Option<PathBuf>,
    subtitles_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
) -> Result<()> {
    let output_path = output_path.unwrap_or_else(|| {
        let mut out = movie.clone();
        out.set_extension("radio-play.mp3");
        out
    });

    let cfg = AnalysisConfig::default();
    let mut timeline = resolve_timeline(&movie, timeline_path, &cfg)?;
    autofill_silent_scene_sfx(&mut timeline);

    let srt_content = subtitles_path.map(std::fs::read_to_string).transpose()?;
    let identifier = GapIdentifier::default();
    let srt_ref: Option<&str> = srt_content.as_deref();
    // skipcq: RS-E1015 — DeepSource false positive: srt_ref is Option<&str>, not unit-type
    let gap_analysis = identifier.identify_gaps(&timeline, srt_ref)?; // skipcq: RS-E1015
    info!(gaps = gap_analysis.gaps.len(), "Identified visual gaps");

    let scripts = if gap_analysis.gaps.is_empty() {
        info!("No gaps found — skipping narration generation");
        Vec::new()
    } else {
        let generator = NarrationGenerator::default();
        let s = generator.generate(&timeline, &gap_analysis.gaps)?;
        info!(scripts = s.len(), "Generated narration scripts");
        s
    };

    let sample_rate = cfg.sample_rate_hz;
    let runtime = tokio::runtime::Runtime::new()?;

    let narration_segments = if scripts.is_empty() {
        Vec::new()
    } else {
        let voice_config = VoiceSynthesisConfig::from_analysis_config(&cfg);
        let language = if voice_config.language.is_empty() {
            "de"
        } else {
            &voice_config.language
        };
        let voice_id = voice_config.voice_id.as_deref();
        let orchestrator = SynthesisOrchestrator::new(voice_config);
        synthesize_narrations(
            &scripts,
            &orchestrator,
            language,
            voice_id,
            &runtime,
            sample_rate,
        )
    };

    let sfx_segments = render_sfx_segments(&timeline, &runtime, sample_rate);

    info!(
        segments = narration_segments.len(),
        sfx_count = sfx_segments.len(),
        "Loading original audio for assembly"
    );
    let (original, _) = decode_audio(&movie, sample_rate)?;
    let assembler = RadioPlayAssembler::new(sample_rate, 50, 0.3);
    let radio_play = assembler.assemble_with_sfx(&original, &narration_segments, &sfx_segments)?;

    write_and_encode_output(&radio_play, sample_rate, &output_path)?;

    info!(
        output = %output_path.display(),
        duration_s = radio_play.len() as f64 / sample_rate as f64,
        "Radio play saved"
    );

    Ok(())
}

fn resolve_timeline(
    movie: &std::path::Path,
    timeline_path: Option<PathBuf>,
    cfg: &AnalysisConfig,
) -> Result<TimelineOutput> {
    if let Some(p) = timeline_path {
        info!(timeline = %p.display(), "Using provided timeline");
        read_timeline(&p)
    } else {
        info!("Extracting timeline from movie");
        extract_timeline(movie, cfg)
    }
}

fn write_and_encode_output(
    samples: &[f32],
    sample_rate: u32,
    output_path: &std::path::Path,
) -> Result<()> {
    let wav_path = output_path.with_extension("tmp.wav");
    write_wav(&wav_path, samples, sample_rate)?;
    encode_to_mp3(&wav_path, output_path)?;
    let _ = std::fs::remove_file(&wav_path);
    Ok(())
}

fn synthesize_narrations(
    scripts: &[NarrationScript],
    orchestrator: &SynthesisOrchestrator,
    language: &str,
    voice_id: Option<&str>,
    runtime: &tokio::runtime::Runtime,
    sample_rate: u32,
) -> Vec<NarrationSegment> {
    let mut narration_segments = Vec::new();
    let assembler = RadioPlayAssembler::new(sample_rate, 50, 0.3);

    for (i, script) in scripts.iter().enumerate() {
        info!(
            i = i + 1,
            total = scripts.len(),
            text = %script.text,
            gap_ms = script.gap_start_ms,
            "Synthesizing narration"
        );

        let request = SynthesisRequest {
            text: script.text.clone(),
            emotion: script.emotion.clone(),
            voice_id: voice_id.map(str::to_string),
            language: language.to_string(),
            speed: 1.0,
            sample_rate_hz: sample_rate,
        };

        match runtime.block_on(orchestrator.synthesize(&request)) {
            Ok(audio) => {
                let segment = assembler.narration_to_segment(script, &audio.samples);
                narration_segments.push(segment);
                info!(
                    i = i + 1,
                    samples = audio.samples.len(),
                    "Narration synthesized"
                );
            }
            Err(e) => {
                tracing::warn!(i = i + 1, error = %e, "TTS failed for this gap, skipping");
            }
        }
    }

    narration_segments
}

fn render_sfx_segments(
    timeline: &TimelineOutput,
    runtime: &tokio::runtime::Runtime,
    sample_rate: u32,
) -> Vec<SfxSegment> {
    let sfx_config = SoundEffectsConfig {
        enabled: true,
        ..Default::default()
    };
    let mut sfx_segments = Vec::new();
    let Ok(sfx_mgr) = SfxManager::from_config(&sfx_config) else {
        return sfx_segments;
    };

    for seg in &timeline.segments {
        let Some(ref trigger) = seg.sfx_trigger else {
            continue;
        };
        if *trigger == SfxTrigger::None {
            continue;
        }
        let duration_secs =
            ((seg.end_ms.saturating_sub(seg.start_ms)) as f32 / 1000.0).clamp(0.0, 300.0);
        match runtime.block_on(sfx_mgr.render_trigger(trigger, sample_rate, Some(duration_secs))) {
            Ok(Some(samples)) => {
                let start_sample = (seg.start_ms as f64 * sample_rate as f64 / 1000.0) as usize;
                sfx_segments.push(SfxSegment {
                    start_sample,
                    samples,
                });
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "Failed to render SFX for trigger, skipping"),
        }
    }

    sfx_segments
}

fn write_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    use hound::{WavSpec, WavWriter};

    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec)?;
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let sample = (clamped * i16::MAX as f32) as i16;
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
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
        bail!("ffmpeg MP3 encoding failed");
    }
    Ok(())
}
