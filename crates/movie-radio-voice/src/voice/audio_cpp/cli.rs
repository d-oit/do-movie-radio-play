use anyhow::{Context, Result};
use std::env;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use super::http::ModelParams;
use super::wav::decode_and_resample_wav;
use super::AudioOutput;
use crate::config::AudioCppConfig;
use crate::voice::SynthesisRequest;

fn rand_id() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345)
}

pub(crate) async fn synthesize_local_cli(
    config: &AudioCppConfig,
    request: &SynthesisRequest,
    params: &ModelParams<'_>,
    timeout_duration: Duration,
) -> Result<AudioOutput> {
    let binary = &config.local.binary;
    let temp_dir = env::temp_dir();
    let temp_filename = format!("audiocpp_out_{}_{}.wav", std::process::id(), rand_id());
    let output_path = temp_dir.join(temp_filename);

    let language = if request.language.is_empty() {
        params.default_language
    } else {
        &request.language
    };

    let mut cmd = Command::new(binary);
    cmd.arg("--model").arg(params.model);
    cmd.arg("--input").arg(&request.text);
    cmd.arg("--output").arg(&output_path);
    cmd.arg("--language").arg(language);
    cmd.arg("--backend").arg(params.backend);
    cmd.arg("--family").arg(params.family);

    if let Some(ref v_id) = request.voice_id {
        cmd.arg("--voice").arg(v_id);
    }
    if let Some(ref v_ref) = config.voice_ref {
        cmd.arg("--voice-ref").arg(v_ref);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let child_res = timeout(timeout_duration, cmd.output()).await;
    let output = match child_res {
        Ok(Ok(res)) => res,
        Ok(Err(e)) => {
            let _ = tokio::fs::remove_file(&output_path).await;
            return Err(anyhow::anyhow!(
                "Failed to execute local audiocpp_cli process: {}",
                e
            ));
        }
        Err(_) => {
            let _ = tokio::fs::remove_file(&output_path).await;
            anyhow::bail!(
                "audiocpp_cli process execution timed out after {:?}",
                timeout_duration
            );
        }
    };

    if !output.status.success() {
        let stderr_text = String::from_utf8_lossy(&output.stderr);
        let _ = tokio::fs::remove_file(&output_path).await;
        anyhow::bail!(
            "audiocpp_cli exited with status {}: {}",
            output.status,
            stderr_text.trim()
        );
    }

    let wav_bytes = tokio::fs::read(&output_path)
        .await
        .context("Failed to read audiocpp_cli WAV output file")?;
    let _ = tokio::fs::remove_file(&output_path).await;

    decode_and_resample_wav(&wav_bytes, request.sample_rate_hz)
}
