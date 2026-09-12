use super::*;
use hound::{WavSpec, WavWriter};

#[test]
fn test_decode_via_symphonia_wav() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("test.wav");

    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&wav_path, spec).unwrap();
    for _ in 0..16000 {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();

    let (samples, _) = symphonia::decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
    assert_eq!(samples.len(), 16000);
    for &s in &samples {
        assert_eq!(s, 0.0);
    }
}

#[test]
fn test_decode_audio_dispatch() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("test.wav");
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&wav_path, spec).unwrap();
    for _ in 0..16000 {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();

    let (samples, sr) = decode_audio(&wav_path, 8000).unwrap();
    assert_eq!(sr, 8000);
    assert!(
        samples.len() == 8000 || samples.len() == 7999,
        "expected ~8000 samples, got {}",
        samples.len()
    );
}

#[test]
fn test_decode_via_symphonia_stereo() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("stereo.wav");

    let spec = WavSpec {
        channels: 2,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&wav_path, spec).unwrap();
    for _ in 0..16000 {
        writer.write_sample(i16::MAX).unwrap();
        writer.write_sample(i16::MIN + 1).unwrap();
    }
    writer.finalize().unwrap();

    let (samples, _) = symphonia::decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
    assert_eq!(samples.len(), 16000);
    for &s in &samples {
        assert!(s.abs() < 1e-4);
    }
}

#[test]
fn test_decode_via_symphonia_wav_24bit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("test24.wav");

    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&wav_path, spec).unwrap();
    let full_scale = (1i32 << 23) - 1;
    writer.write_sample(full_scale).unwrap();
    writer.write_sample(-full_scale).unwrap();
    writer.write_sample(0i32).unwrap();
    writer.finalize().unwrap();

    let (samples, _) = symphonia::decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
    assert_eq!(samples.len(), 3);
    assert!((samples[0] - 1.0).abs() < 1e-6, "got {}", samples[0]);
    assert!((samples[1] + 1.0).abs() < 1e-6, "got {}", samples[1]);
    assert_eq!(samples[2], 0.0);
}

#[test]
fn test_decode_via_symphonia_wav_32bit_float() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("test32f.wav");

    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = WavWriter::create(&wav_path, spec).unwrap();
    writer.write_sample(1.0f32).unwrap();
    writer.write_sample(-1.0f32).unwrap();
    writer.write_sample(0.25f32).unwrap();
    writer.finalize().unwrap();

    let (samples, _) = symphonia::decode_via_symphonia(&wav_path, Some("wav"), 16000).unwrap();
    assert_eq!(samples.len(), 3);
    assert!((samples[0] - 1.0).abs() < 1e-6, "got {}", samples[0]);
    assert!((samples[1] + 1.0).abs() < 1e-6, "got {}", samples[1]);
    assert!((samples[2] - 0.25).abs() < 1e-6, "got {}", samples[2]);
}

#[test]
fn test_decode_audio_fallback_disabled_propagates_symphonia_error() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("corrupt.wav");
    std::fs::write(&wav_path, b"not a wav file").unwrap();

    let err = decode_audio_with_fallback(&wav_path, 16000, false)
        .unwrap_err()
        .to_string();
    assert!(
        !err.contains("fallback is disabled"),
        "expected the symphonia error, got: {err}"
    );
}

#[test]
fn test_decode_audio_fallback_disabled_rejects_unsupported_ext() {
    let temp_dir = tempfile::tempdir().unwrap();
    let xyz_path = temp_dir.path().join("test.xyz");
    std::fs::write(&xyz_path, b"dummy content").unwrap();

    let err = decode_audio_with_fallback(&xyz_path, 16000, false)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("fallback is disabled"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_decode_audio_chunks_cb_streaming() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wav_path = temp_dir.path().join("chunk_test.wav");
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&wav_path, spec).unwrap();
    // 3 seconds of audio = 48,000 samples
    for i in 0..48000 {
        let sample = ((i % 100) as f32 / 100.0 * i16::MAX as f32) as i16;
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();

    let mut chunk_count = 0;
    let mut total_samples = 0;

    decode_audio_chunks_cb(&wav_path, 16000, 1, |chunk_samples, idx| {
        assert_eq!(chunk_count, idx);
        chunk_count += 1;
        total_samples += chunk_samples.len();
        Ok(())
    })
    .unwrap();

    assert_eq!(chunk_count, 3);
    assert_eq!(total_samples, 48000);
}
