use anyhow::Result;
use std::path::PathBuf;

pub fn handle_voice_samples(
    character: String,
    input: PathBuf,
    output: Option<PathBuf>,
) -> Result<()> {
    // Deterministic sample extraction placeholder: validates inputs without shell.
    if character.trim().is_empty() {
        anyhow::bail!("character must not be empty");
    }
    if input.to_string_lossy().contains("..") {
        anyhow::bail!("input path must not contain ..");
    }
    let out = output.unwrap_or_else(|| PathBuf::from(format!("voice_samples/{character}.json")));
    println!(
        "voice samples for {character} from {} -> {} (dry-run, deterministic)",
        input.display(),
        out.display()
    );
    println!("runtime=audio_cpp family=qwen3_tts — capability check: voice cloning supported if model allows reference_audio");
    Ok(())
}

pub fn handle_voice_list() -> Result<()> {
    println!("voice references: (none stored yet) — use `voice samples --character NAME --input movie.mkv`");
    Ok(())
}

pub fn handle_voice_test(character: String, text: String) -> Result<()> {
    use crate::app_config_loader::load_app_config;
    let cfg = load_app_config(None)?;
    println!("voice test character={character} text={text:?} runtime={} endpoint={} (no secrets printed)", cfg.voice_clone.runtime, if cfg.voice.audio_cpp.remote.server_url.is_empty() { "local" } else { "remote" });
    Ok(())
}
