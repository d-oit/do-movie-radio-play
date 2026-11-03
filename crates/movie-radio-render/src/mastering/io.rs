//! Audio file reading/writing and report serialization.

use anyhow::{bail, Context, Result};
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use super::profiles::MasteringProfile;
use super::report::{validate, CheckStatus, MasteringReport};

pub fn validate_file(path: &Path, profile: &MasteringProfile) -> Result<MasteringReport> {
    let (samples, sample_rate, channels) = read_audio(path)?;
    Ok(validate(&samples, sample_rate, channels, profile))
}

/// Reads WAV natively (preserving the interleaved channel layout) and other
/// supported delivery containers through Symphonia.
pub fn read_audio(path: &Path) -> Result<(Vec<f32>, u32, u16)> {
    let is_wav = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"));
    if is_wav {
        read_wav(path)
    } else {
        decode_compressed(path)
    }
}

fn decode_compressed(path: &Path) -> Result<(Vec<f32>, u32, u16)> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let track = format
        .default_track(TrackType::Audio)
        .context("no audio track found")?;
    let codec_params = track
        .codec_params
        .as_ref()
        .context("no codec params")?
        .audio()
        .context("not an audio track")?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())?;
    let track_id = track.id;

    let mut samples = Vec::new();
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        let frames = decoded.frames();
        let decoded_channels = decoded.spec().channels().count();
        if frames == 0 || decoded_channels == 0 {
            continue;
        }
        channels = decoded_channels as u16;
        sample_rate = decoded.spec().rate();
        let mut buf = vec![0.0f32; frames * decoded_channels];
        decoded.copy_to_slice_interleaved(&mut buf);
        samples.extend_from_slice(&buf);
    }
    if samples.is_empty() {
        bail!("decoded audio is empty: {}", path.display());
    }
    Ok((samples, sample_rate, channels))
}

pub fn read_wav(path: &Path) -> Result<(Vec<f32>, u32, u16)> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let spec = reader.spec();
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 8) => reader
            .samples::<i8>()
            .map(|s| s.map(|v| f32::from(v) / 128.0))
            .collect::<std::result::Result<_, _>>()?,
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|s| s.map(|v| f32::from(v) / 32_768.0))
            .collect::<std::result::Result<_, _>>()?,
        (hound::SampleFormat::Int, 24) => reader
            .samples::<i32>()
            .map(|s| s.map(|v| v as f32 / 8_388_608.0))
            .collect::<std::result::Result<_, _>>()?,
        (hound::SampleFormat::Int, 32) => reader
            .samples::<i32>()
            .map(|s| s.map(|v| v as f32 / 2_147_483_648.0))
            .collect::<std::result::Result<_, _>>()?,
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<std::result::Result<_, _>>()?,
        (format, bits) => bail!("unsupported WAV format: {format:?} {bits}-bit"),
    };
    Ok((samples, spec.sample_rate, spec.channels))
}

pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32, channels: u16) -> Result<()> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("cannot create {}", path.display()))?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

pub fn write_json(path: &Path, report: &MasteringReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, serde_json::to_vec_pretty(report)?)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn status_class(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "pass",
        CheckStatus::Warn => "warn",
        CheckStatus::Fail => "fail",
    }
}

fn status_label(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "PASS",
        CheckStatus::Warn => "WARN",
        CheckStatus::Fail => "FAIL",
    }
}

pub fn write_html(path: &Path, report: &MasteringReport) -> Result<()> {
    let mut html = String::new();
    html.push_str(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>Mastering quality report</title><style>\
         body{font-family:system-ui,sans-serif;margin:2rem;color:#1f2933}\
         table{border-collapse:collapse;width:100%;margin-bottom:1.5rem}\
         th,td{border:1px solid #cbd2d9;padding:.4rem .6rem;text-align:left}\
         th{background:#f0f4f8}.pass{color:#0b7a3b;font-weight:600}\
         .warn{color:#a15c00;font-weight:600}.fail{color:#b42318;font-weight:600}\
         </style></head><body>",
    );
    html.push_str(&format!(
        "<h1>Mastering quality report</h1><p>Profile: <strong>{}</strong> \
         (target {:.1} LUFS, true-peak ceiling {:.1} dBTP)</p>",
        escape_html(&report.profile.name),
        report.profile.target_lufs,
        report.profile.max_true_peak_dbtp
    ));
    html.push_str(&format!(
        "<p>Overall: <span class=\"{}\">{}</span></p>",
        status_class(report.overall),
        status_label(report.overall)
    ));
    html.push_str(&format!(
        "<p>Applied gain: {:.2} dB; limiter reduction: {:.2} dB</p>",
        report.applied_gain_db, report.limiting_reduction_db
    ));

    html.push_str("<h2>Measurements</h2><table><tr><th>Metric</th><th>Value</th></tr>");
    let m = &report.measurements;
    let rows = [
        (
            "Integrated loudness",
            format!("{:.1} LUFS", m.integrated_lufs),
        ),
        ("Loudness range", format!("{:.1} LU", m.loudness_range_lu)),
        ("Momentary max", format!("{:.1} LUFS", m.momentary_max_lufs)),
        (
            "Short-term max",
            format!("{:.1} LUFS", m.short_term_max_lufs),
        ),
        ("Sample peak", format!("{:.2} dBFS", m.sample_peak_dbfs)),
        ("True peak", format!("{:.2} dBTP", m.true_peak_dbtp)),
        ("DC offset", format!("{:.5}", m.dc_offset)),
        ("Clipped samples", m.clipped_samples.to_string()),
        ("Longest silence", format!("{} ms", m.longest_silence_ms)),
        ("Max discontinuity", format!("{:.3}", m.max_discontinuity)),
        ("Max gain change", format!("{:.1} LU", m.max_gain_change_lu)),
        (
            "Channel imbalance",
            format!("{:.2} dB", m.channel_imbalance_db),
        ),
        ("Duration", format!("{:.2} s", m.duration_s)),
    ];
    for (label, value) in rows {
        html.push_str(&format!("<tr><td>{label}</td><td>{value}</td></tr>"));
    }
    html.push_str("</table>");

    html.push_str(
        "<h2>Checks</h2><table><tr><th>Check</th><th>Status</th>\
         <th>Measured</th><th>Threshold</th><th>Message</th></tr>",
    );
    for check in &report.checks {
        html.push_str(&format!(
            "<tr><td>{}</td><td class=\"{}\">{}</td><td>{:.3} {}</td>\
             <td>{:.3} {}</td><td>{}</td></tr>",
            escape_html(&check.id),
            status_class(check.status),
            status_label(check.status),
            check.measured,
            escape_html(&check.unit),
            check.threshold,
            escape_html(&check.unit),
            escape_html(&check.message)
        ));
    }
    html.push_str("</table></body></html>");

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, html).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}
