use anyhow::{Context, Result};
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

/// Tolerance in milliseconds for synthetic test profiles.
pub const TOLERANCE_SYNTHETIC_MS: u64 = 100;
/// Tolerance in milliseconds for dataset test profiles.
pub const TOLERANCE_DATASET_MS: u64 = 200;
/// Default tolerance in milliseconds for unknown/other profiles.
pub const TOLERANCE_DEFAULT_MS: u64 = 400;

/// Tolerance values for validation profiles.
pub fn tolerance_for_profile(profile: &str) -> u64 {
    match profile {
        "synthetic" => TOLERANCE_SYNTHETIC_MS,
        "dataset" => TOLERANCE_DATASET_MS,
        _ => TOLERANCE_DEFAULT_MS,
    }
}

/// Initialize tracing-based logging.
pub fn init_logging() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::from(Level::INFO.as_str()));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

/// Open a file path in the system default browser.
pub fn open_in_browser(path: &std::path::Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let path_str = absolute.to_string_lossy().to_string();

    if is_wsl() && try_open_wsl(&path_str, &absolute)? {
        return Ok(());
    }

    if cfg!(target_os = "macos") {
        try_open_macos(&path_str, &absolute)
    } else if cfg!(target_os = "windows") {
        try_open_windows(&path_str, &absolute)
    } else {
        try_open_linux(&path_str, &absolute)
    }
}

/// Try browser openers under WSL.
fn try_open_wsl(path_str: &str, absolute: &std::path::Path) -> Result<bool> {
    if try_open("wslview", &[path_str])? {
        info!(path = %absolute.display(), opener = "wslview", "opened review output in browser");
        return Ok(true);
    }

    if let Some(win_path) = wsl_to_windows_path(path_str)? {
        // Use explorer.exe directly to avoid cmd.exe shell parsing risks (RS-W1051)
        if try_open("explorer.exe", &[&win_path])? {
            info!(path = %absolute.display(), opener = "explorer.exe", "opened review output in browser");
            return Ok(true);
        }
        // Harden powershell call by using EncodedCommand and LiteralPath to prevent command injection
        let encoded_cmd = build_powershell_encoded_cmd(&win_path);
        if try_open(
            "powershell.exe",
            &["-NoProfile", "-EncodedCommand", &encoded_cmd],
        )? {
            info!(path = %absolute.display(), opener = "powershell.exe", "opened review output in browser");
            return Ok(true);
        }
    }
    Ok(false)
}

/// Try browser openers on macOS.
fn try_open_macos(path_str: &str, absolute: &std::path::Path) -> Result<()> {
    if try_open("open", &[path_str])? {
        info!(path = %absolute.display(), opener = "open", "opened review output in browser");
        return Ok(());
    }
    anyhow::bail!(
        "could not auto-open browser for {}; open it manually",
        absolute.display()
    )
}

/// Try browser openers on Windows.
fn try_open_windows(path_str: &str, absolute: &std::path::Path) -> Result<()> {
    // Use explorer directly to avoid cmd shell parsing risks (RS-W1051)
    if try_open("explorer", &[path_str])? {
        info!(path = %absolute.display(), opener = "explorer", "opened review output in browser");
        return Ok(());
    }
    // Harden powershell call by using EncodedCommand and LiteralPath to prevent command injection
    let encoded_cmd = build_powershell_encoded_cmd(path_str);
    if try_open(
        "powershell",
        &["-NoProfile", "-EncodedCommand", &encoded_cmd],
    )? {
        info!(path = %absolute.display(), opener = "powershell", "opened review output in browser");
        return Ok(());
    }
    anyhow::bail!(
        "could not auto-open browser for {}; open it manually",
        absolute.display()
    )
}

/// Build a Base64 encoded UTF-16LE command string for PowerShell `-EncodedCommand`.
/// Using `-LiteralPath` ensures wildcards and special characters in paths are not evaluated as patterns.
fn build_powershell_encoded_cmd(path: &str) -> String {
    let script = format!("Start-Process -LiteralPath '{}'", path.replace('\'', "''"));
    let utf16: Vec<u16> = script.encode_utf16().collect();
    let bytes: Vec<u8> = utf16.into_iter().flat_map(|u| u.to_le_bytes()).collect();
    encode_base64(&bytes)
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut res = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        let b24 = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        res.push(TABLE[((b24 >> 18) & 63) as usize] as char);
        res.push(TABLE[((b24 >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            res.push(TABLE[((b24 >> 6) & 63) as usize] as char);
        } else {
            res.push('=');
        }
        if chunk.len() > 2 {
            res.push(TABLE[(b24 & 63) as usize] as char);
        } else {
            res.push('=');
        }
    }
    res
}

/// Try browser openers on Linux.
fn try_open_linux(path_str: &str, absolute: &std::path::Path) -> Result<()> {
    let file_url = format!("file://{}", absolute.display());

    if let Some(default_browser) = linux_default_browser_command()? {
        if try_open(&default_browser, &["--new-window", &file_url])?
            || try_open(&default_browser, &[&file_url])?
        {
            info!(
                path = %absolute.display(),
                opener = %default_browser,
                "opened review output in browser"
            );
            return Ok(());
        }
    }

    if let Some(browser_env) = std::env::var_os("BROWSER") {
        let browser_env = browser_env.to_string_lossy().to_string();
        for candidate in browser_env.split(':').filter(|c| !c.is_empty()) {
            if try_open(candidate, &[&file_url])? {
                info!(path = %absolute.display(), opener = %candidate, "opened review output in browser");
                return Ok(());
            }
        }
    }

    for candidate in ["google-chrome", "chromium-browser", "chromium", "firefox"] {
        if try_open(candidate, &["--new-window", &file_url])? || try_open(candidate, &[&file_url])?
        {
            info!(path = %absolute.display(), opener = %candidate, "opened review output in browser");
            return Ok(());
        }
    }

    if try_open("xdg-open", &[path_str])? {
        info!(path = %absolute.display(), opener = "xdg-open", "opened review output in browser");
        return Ok(());
    }
    if try_open("gio", &["open", path_str])? {
        info!(path = %absolute.display(), opener = "gio open", "opened review output in browser");
        return Ok(());
    }

    anyhow::bail!(
        "could not auto-open browser for {}; open it manually",
        absolute.display()
    )
}

fn linux_default_browser_command() -> Result<Option<String>> {
    let output = match std::process::Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(_) => return Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("failed running xdg-settings"),
    };

    let desktop = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if desktop.is_empty() {
        return Ok(None);
    }

    let command = desktop.strip_suffix(".desktop").unwrap_or(&desktop).trim();
    if command.is_empty() {
        Ok(None)
    } else {
        Ok(Some(command.to_string()))
    }
}

fn try_open(program: &str, args: &[&str]) -> Result<bool> {
    match std::process::Command::new(program).args(args).status() {
        Ok(status) => Ok(status.success()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| format!("failed to run browser opener: {program}")),
    }
}

fn wsl_to_windows_path(path: &str) -> Result<Option<String>> {
    match std::process::Command::new("wslpath")
        .args(["-w", path])
        .output()
    {
        Ok(output) if output.status.success() => {
            let win = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if win.is_empty() {
                Ok(None)
            } else {
                Ok(Some(win))
            }
        }
        Ok(_) => Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).context("failed running wslpath"),
    }
}

fn is_wsl() -> bool {
    if std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some() {
        return true;
    }
    if let Ok(version) = std::fs::read_to_string("/proc/version") {
        let lower = version.to_ascii_lowercase();
        return lower.contains("microsoft") || lower.contains("wsl");
    }
    false
}

const ENV_APPDATA: &str = "APPDATA";
const ENV_HOME: &str = "HOME";
const ENV_XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";

/// Get the calibration profiles directory path.
pub fn get_calibration_dir() -> Result<std::path::PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var(ENV_APPDATA)?
    } else if cfg!(target_os = "macos") {
        std::path::PathBuf::from(std::env::var(ENV_HOME)?)
            .join("Library/Application Support")
            .to_string_lossy()
            .to_string()
    } else {
        std::env::var(ENV_XDG_CONFIG_HOME)
            .or_else(|_| std::env::var(ENV_HOME).map(|h| format!("{h}/.config")))
            .map_err(|_| anyhow::anyhow!("Neither XDG_CONFIG_HOME nor HOME set"))?
    };
    Ok(std::path::PathBuf::from(base).join("do-movie-radio-play/profiles"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_powershell_encoded_cmd() {
        let path = "C:\\path\\to'file\\test[1].html";
        let encoded = build_powershell_encoded_cmd(path);
        assert!(!encoded.is_empty());

        let expected_script = "Start-Process -LiteralPath 'C:\\path\\to''file\\test[1].html'";
        let utf16: Vec<u16> = expected_script.encode_utf16().collect();
        let bytes: Vec<u8> = utf16.into_iter().flat_map(|u| u.to_le_bytes()).collect();
        let expected_encoded = encode_base64(&bytes);

        assert_eq!(encoded, expected_encoded);
    }
}
