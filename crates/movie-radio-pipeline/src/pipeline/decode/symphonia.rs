use anyhow::{Context, Result};
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use movie_radio_types::TimelineError;

pub fn decode_via_symphonia(
    path: &Path,
    extension: Option<&str>,
    target_sample_rate: u32,
) -> Result<(Vec<f32>, u32)> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = extension {
        hint.with_extension(ext);
    }

    let mut format_reader = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;

    let track = format_reader
        .default_track(TrackType::Audio)
        .context("no audio track found")?;

    let codec_params = track
        .codec_params
        .as_ref()
        .context("no codec params")?
        .audio()
        .context("not audio")?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())?;

    let track_id = track.id;
    let source_sample_rate = codec_params.sample_rate.context("unknown sample rate")?;

    let mut samples = Vec::new();
    let mut decode_buf: Vec<f32> = Vec::new();

    loop {
        let packet = match format_reader.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(_) => break,
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet)?;
        let frames = decoded.frames();
        let channels = decoded.spec().channels().count();

        decode_buf.resize(frames * channels, 0.0);
        decoded.copy_to_slice_interleaved(&mut decode_buf);

        for frame in decode_buf.chunks_exact(channels) {
            let mono_sample: f32 = frame.iter().sum::<f32>() / channels as f32;
            samples.push(mono_sample);
        }
    }

    if samples.is_empty() {
        return Err(TimelineError::EmptyAudio.into());
    }

    let resampled =
        crate::pipeline::resample::resample(&samples, source_sample_rate, target_sample_rate)?;
    Ok((resampled, target_sample_rate))
}
