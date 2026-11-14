//! Streaming assembly with SFX mixing.
//!
//! `assemble_with_sfx` holds the ducked source and then the summed mix at full
//! length. These helpers produce the same bytes one chunk at a time, so peak
//! memory stays bounded for long films (#418).

use anyhow::Result;
use movie_radio_verification::SampleSource;
use std::io::Write;

use movie_radio_render::agc::StreamingAgc;
use movie_radio_render::spatial::StereoPosition;

use crate::assemble::{
    add_sfx_segment, NarrationSegment, RadioPlayAssembler, SfxSegment, ASSEMBLY_CHUNK_SAMPLES,
    MIX_AGC_ATTACK_SECS, MIX_AGC_MAIN_MAX_GAIN, MIX_AGC_RELEASE_SECS, MIX_AGC_SFX_MAX_GAIN,
};

impl RadioPlayAssembler {
    /// Streams the ducked mix to `writer` as raw little-endian `f32`.
    ///
    /// Equivalent to [`RadioPlayAssembler::assemble`] but holds one chunk
    /// instead of a full-length copy of the source, so peak memory does not
    /// scale with film length (#418).
    pub fn assemble_to_writer<S: SampleSource + ?Sized, W: Write>(
        &self,
        original: &S,
        narrations: &[NarrationSegment],
        writer: &mut W,
    ) -> Result<()> {
        self.validate_no_overlaps(narrations)?;
        let total_len = original.len();
        let mut chunk = vec![0.0f32; ASSEMBLY_CHUNK_SAMPLES];
        let mut window = Vec::with_capacity(ASSEMBLY_CHUNK_SAMPLES);
        let mut bytes = Vec::new();
        let mut start = 0usize;
        while start < total_len {
            let end = (start + ASSEMBLY_CHUNK_SAMPLES).min(total_len);
            let buffer = &mut chunk[..end - start];
            original.read_window(start, end, &mut window)?;
            buffer.copy_from_slice(&window);
            self.duck_chunk(narrations, start, buffer, total_len);

            bytes.clear();
            bytes.reserve(buffer.len() * 4);
            for sample in buffer.iter() {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            writer.write_all(&bytes)?;
            start = end;
        }
        Ok(())
    }

    /// Applies narration ducking in place to `[chunk_start, chunk_start + buffer.len())`.
    ///
    /// Ducking only rewrites a window around each narration, so a chunk needs
    /// only the narrations that overlap it — that is what lets the streaming
    /// path avoid a full-length copy.
    fn duck_chunk(
        &self,
        narrations: &[NarrationSegment],
        chunk_start: usize,
        buffer: &mut [f32],
        total_len: usize,
    ) {
        let chunk_end = chunk_start + buffer.len();
        for narration in narrations {
            let narr_start = narration.start_sample.min(total_len);
            let narr_end = narration.end_sample.min(total_len);
            let narr_len = narr_end.saturating_sub(narr_start);
            if narr_len == 0 {
                continue;
            }
            let samples = &narration.samples[..narr_len.min(narration.samples.len())];
            let span_end = narr_start + samples.len();
            let lo = narr_start.max(chunk_start);
            let hi = span_end.min(chunk_end);
            if lo >= hi {
                continue;
            }
            for pos in lo..hi {
                let i = pos - narr_start;
                let (duck_ramp, envelope) = self.duck_envelope(i, samples.len());
                let slot = &mut buffer[pos - chunk_start];
                *slot = *slot * duck_ramp + samples[i] * envelope;
            }
        }
    }

    /// Streams the SFX-mixed radio play to `writer` as raw little-endian `f32`.
    ///
    /// Equivalent to [`RadioPlayAssembler::assemble_with_sfx`]: same ducking,
    /// same AGC per track, same summation, and the same `SQRT_2` fold with the
    /// same peak normalization, applied in the same order so the bytes match.
    pub fn assemble_with_sfx_to_writer<S: SampleSource + ?Sized, W: Write>(
        &self,
        original: &S,
        narrations: &[NarrationSegment],
        sfx_segments: &[SfxSegment],
        writer: &mut W,
    ) -> Result<()> {
        // `assemble_with_sfx` returns the ducked mix untouched when there are
        // no effects, with no AGC, fold or normalization.
        if sfx_segments.is_empty() {
            return self.assemble_to_writer(original, narrations, writer);
        }

        self.validate_no_overlaps(narrations)?;
        let total_len = original.len();
        let sfx_len = sfx_segments
            .iter()
            .map(|seg| seg.start_sample.saturating_add(seg.samples.len()))
            .max()
            .unwrap_or(0)
            .min(total_len);
        let mut sfx_track = vec![0.0f32; sfx_len];
        for seg in sfx_segments {
            add_sfx_segment(&mut sfx_track, seg);
        }

        // Peak normalization needs the whole mix, so measure first and write
        // second. Both passes run the same deterministic per-track AGC.
        let mut peak = 0.0f32;
        self.for_each_mixed_chunk(original, narrations, &sfx_track, |chunk| {
            for sample in chunk {
                peak = peak.max(sample.abs());
            }
            Ok(())
        })?;

        let scale = if peak > 1.0 { 1.0 / peak } else { 1.0 };
        let mut bytes = Vec::new();
        self.for_each_mixed_chunk(original, narrations, &sfx_track, |chunk| {
            bytes.clear();
            bytes.reserve(chunk.len() * 4);
            for &sample in chunk {
                let folded = sample * std::f32::consts::SQRT_2;
                let scaled = if peak > 1.0 { folded * scale } else { folded };
                bytes.extend_from_slice(&scaled.to_le_bytes());
            }
            writer.write_all(&bytes)?;
            Ok(())
        })
    }

    /// Calls `visit` with each mixed chunk, in order.
    fn for_each_mixed_chunk<S: SampleSource + ?Sized, F>(
        &self,
        original: &S,
        narrations: &[NarrationSegment],
        sfx_track: &[f32],
        mut visit: F,
    ) -> Result<()>
    where
        F: FnMut(&[f32]) -> Result<()>,
    {
        let total_len = original.len();
        // Every track in the centre-mono mix is scaled by the centre pan gain,
        // then the summed mix is folded back up by `SQRT_2`.
        let pan_gain = StereoPosition::CENTRE.gains().0;
        let mut main_agc = StreamingAgc::new(
            self.sample_rate,
            MIX_AGC_ATTACK_SECS,
            MIX_AGC_RELEASE_SECS,
            MIX_AGC_MAIN_MAX_GAIN,
        )?;
        let mut sfx_agc = StreamingAgc::new(
            self.sample_rate,
            MIX_AGC_ATTACK_SECS,
            MIX_AGC_RELEASE_SECS,
            MIX_AGC_SFX_MAX_GAIN,
        )?;
        let mut chunk = vec![0.0f32; ASSEMBLY_CHUNK_SAMPLES];
        let mut window = Vec::with_capacity(ASSEMBLY_CHUNK_SAMPLES);
        let mut sfx_buf = vec![0.0f32; ASSEMBLY_CHUNK_SAMPLES];
        let mut start = 0usize;
        while start < total_len {
            let end = (start + ASSEMBLY_CHUNK_SAMPLES).min(total_len);
            let buffer = &mut chunk[..end - start];
            original.read_window(start, end, &mut window)?;
            buffer.copy_from_slice(&window);
            self.duck_chunk(narrations, start, buffer, total_len);
            main_agc.process(buffer);
            for sample in buffer.iter_mut() {
                *sample *= pan_gain;
            }

            // The SFX track ends at its last effect; past that only the main
            // track contributes, exactly as the mixer's length clamp does.
            let sfx_hi = end.min(sfx_track.len());
            if start < sfx_hi {
                let count = sfx_hi - start;
                sfx_buf[..count].copy_from_slice(&sfx_track[start..sfx_hi]);
                sfx_agc.process(&mut sfx_buf[..count]);
                for (out, effect) in buffer[..count].iter_mut().zip(&sfx_buf[..count]) {
                    *out += *effect * pan_gain;
                }
            }

            visit(buffer)?;
            start = end;
        }
        Ok(())
    }
}
