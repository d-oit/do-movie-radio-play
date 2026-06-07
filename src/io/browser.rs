use anyhow::{bail, Context, Result};
use std::path::Path;
use tracing::info;

pub fn open_in_browser(path: &Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let path_str = absolute.to_string_lossy().to_string();

    if is_wsl() {
        if try_open("wslview", &[&path_str])? {
            info!(path = %absolute.display(), opener = "wslview", "opened review output in browser");
            return Ok(());
        }

        if let Some(win_path) = wsl_to_windows_path(&path_str)? {
            if try_open("cmd.exe", &["/C", "start", "", &win_path])? {
                info!(path = %absolute.display(), opener = "cmd.exe/start", "opened review output in browser");
                return Ok(());
            }
            if try_open(
                "powershell.exe",
                &["-NoProfile", "-Command", "Start-Process", &win_path],
            )? {
                info!(path = %absolute.display(), opener = "powershell.exe", "opened review output in browser");
                return Ok(());
            }
        }
    }

    if cfg!(target_os = "macos") {
        if try_open("open", &[&path_str])? {
            info!(path = %absolute.display(), opener = "open", "opened review output in browser");
            return Ok(());
        }
    } else if cfg!(target_os = "windows") {
        if try_open("cmd", &["/C", "start", "", &path_str])? {
            info!(path = %absolute.display(), opener = "cmd/start", "opened review output in browser");
            return Ok(());
        }
        if try_open(
            "powershell",
            &["-NoProfile", "-Command", "Start-Process", &path_str],
        )? {
            info!(path = %absolute.display(), opener = "powershell", "opened review output in browser");
            return Ok(());
        }
    } else {
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
            if try_open(candidate, &["--new-window", &file_url])?
                || try_open(candidate, &[&file_url])?
            {
                info!(path = %absolute.display(), opener = %candidate, "opened review output in browser");
                return Ok(());
            }
        }

        if try_open("xdg-open", &[&path_str])? {
            info!(path = %absolute.display(), opener = "xdg-open", "opened review output in browser");
            return Ok(());
        }
        if try_open("gio", &["open", &path_str])? {
            info!(path = %absolute.display(), opener = "gio open", "opened review output in browser");
            return Ok(());
        }
    }

    bail!(
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
