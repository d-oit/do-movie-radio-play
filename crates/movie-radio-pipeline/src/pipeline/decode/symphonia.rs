use anyhow::{Context, Result};
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use movie_radio_types::TimelineError;

struct SymphoniaStream {
    reader: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::audio::AudioDecoder>,
    track_id: u32,
    source_sample_rate: u32,
}

fn open_stream(path: &Path, extension: Option<&str>) -> Result<SymphoniaStream> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = extension {
        hint.with_extension(ext);
    }

    let reader = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;

    let track = reader
        .default_track(TrackType::Audio)
        .context("no audio track found")?;
    let codec_params = track
        .codec_params
        .as_ref()
        .context("no codec params")?
        .audio()
        .context("not audio")?;
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())?;
    let track_id = track.id;
    let source_sample_rate = codec_params.sample_rate.context("unknown sample rate")?;

    Ok(SymphoniaStream {
        reader,
        decoder,
        track_id,
        source_sample_rate,
    })
}

/// Decodes in packets and hands resampled mono chunks to `callback`, never
/// holding the whole decoded track. Output matches [`decode_via_symphonia`]
/// sample for sample, including the chunk split.
#[cfg(not(feature = "high-quality-resample"))]
pub fn stream_via_symphonia_chunks<F>(
    path: &Path,
    extension: Option<&str>,
    target_sample_rate: u32,
    chunk_duration_sec: u64,
    callback: &mut F,
) -> Result<()>
where
    F: FnMut(&[f32], usize) -> Result<()>,
{
    let samples_per_chunk = (chunk_duration_sec * target_sample_rate as u64) as usize;
    if samples_per_chunk == 0 {
        return Err(TimelineError::Decode("Chunk duration must be > 0".to_string()).into());
    }

    let mut stream = open_stream(path, extension)?;
    let mut resampler = crate::pipeline::resample::StreamingResampler::new(
        stream.source_sample_rate,
        target_sample_rate,
    );
    let mut pending: Vec<f32> = Vec::with_capacity(samples_per_chunk);
    let mut chunk_idx = 0usize;
    let mut total_samples = 0u64;
    let mut decode_buf: Vec<f32> = Vec::new();

    while let Ok(Some(packet)) = stream.reader.next_packet() {
        if packet.track_id != stream.track_id {
            continue;
        }

        let decoded = stream.decoder.decode(&packet)?;
        let frames = decoded.frames();
        let channels = decoded.spec().channels().count();
        decode_buf.resize(frames * channels, 0.0);
        decoded.copy_to_slice_interleaved(&mut decode_buf);

        let mono: Vec<f32> = decode_buf
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();
        pending.extend(resampler.push(&mono));

        while pending.len() >= samples_per_chunk {
            let rest = pending.split_off(samples_per_chunk);
            callback(&pending, chunk_idx)?;
            chunk_idx += 1;
            pending = rest;
        }
    }

    pending.extend(resampler.finish());
    if !pending.is_empty() {
        callback(&pending, chunk_idx)?;
    }
    total_samples += chunk_idx as u64 * samples_per_chunk as u64 + pending.len() as u64;
    if total_samples == 0 {
        return Err(TimelineError::EmptyAudio.into());
    }
    Ok(())
}

pub fn decode_via_symphonia(
    path: &Path,
    extension: Option<&str>,
    target_sample_rate: u32,
) -> Result<(Vec<f32>, u32)> {
    let mut stream = open_stream(path, extension)?;
    let track_id = stream.track_id;
    let source_sample_rate = stream.source_sample_rate;

    let mut samples = Vec::new();
    let mut decode_buf: Vec<f32> = Vec::new();

    loop {
        let packet = match stream.reader.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(_) => break,
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = stream.decoder.decode(&packet)?;
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
