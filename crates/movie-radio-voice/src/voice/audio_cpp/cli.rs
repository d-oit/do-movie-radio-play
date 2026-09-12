use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tempfile::Builder;
use tokio::process::Command;
use tokio::time::timeout;

use super::http::ModelParams;
use super::wav::decode_and_resample_wav;
use super::AudioOutput;
use crate::config::AudioCppConfig;
use crate::voice::{is_valid_voice_id, SynthesisRequest, SynthesisValidationError};

pub(crate) async fn synthesize_local_cli(
    config: &AudioCppConfig,
    request: &SynthesisRequest,
    params: &ModelParams<'_>,
    timeout_duration: Duration,
) -> Result<AudioOutput> {
    let binary = &config.local.binary;
    let temp_file = Builder::new()
        .prefix("audiocpp_out_")
        .suffix(".wav")
        .tempfile()
        .context("Failed to create secure temporary WAV file")?;
    let output_path = temp_file.path().to_path_buf();

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

    let effective_voice = request
        .voice_id
        .as_deref()
        .unwrap_or_else(|| config.voice_id.as_deref().unwrap_or(""));
    if !effective_voice.is_empty() {
        if !is_valid_voice_id(effective_voice) {
            return Err(SynthesisValidationError::InvalidVoiceId.into());
        }
        cmd.arg("--voice").arg(effective_voice);
    }

    let effective_voice_ref = request
        .reference_audio
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| config.voice_ref.clone());

    if let Some(ref v_ref) = effective_voice_ref {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cli_rejects_invalid_voice_id() {
        let config = AudioCppConfig::default();
        let request = SynthesisRequest {
            voice_id: Some("../admin/path".to_string()),
            ..SynthesisRequest::default()
        };
        let params = ModelParams {
            family: "bark",
            model: "bark-small",
            backend: "cpu",
            default_language: "de",
        };

        let res = synthesize_local_cli(&config, &request, &params, Duration::from_secs(5)).await;

        assert!(res.is_err());
        let err = res.err().unwrap();
        assert_eq!(
            err.downcast::<SynthesisValidationError>().unwrap(),
            SynthesisValidationError::InvalidVoiceId
        );
    }
}
