use std::{
    env,
    io::Write,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow};

const KEYCHAIN_SERVICE: &str = "dev.repotrek.github-token";
const KEYCHAIN_ACCOUNT: &str = "repotrek";

#[must_use]
pub fn token_from_environment(anonymous: bool) -> Option<String> {
    if anonymous {
        return None;
    }
    ["REPOTREK_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find_map(|name| env::var(name).ok())
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

#[must_use]
pub fn token_from_github_cli(anonymous: bool) -> Option<String> {
    if anonymous { None } else { read_gh_token() }
}

pub fn authenticate_with_github_cli() -> Result<String> {
    if let Some(token) = read_gh_token() {
        return Ok(token);
    }
    ensure_gh_installed()?;

    let status = Command::new("gh")
        .args([
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--web",
            "--git-protocol",
            "ssh",
            "--skip-ssh-key",
        ])
        .status()
        .context("Could not start GitHub CLI")?;

    if !status.success() {
        return Err(anyhow!("GitHub authentication did not complete"));
    }

    read_gh_token().ok_or_else(|| anyhow!("Could not obtain an access token from GitHub CLI"))
}

/// Retain a Personal Access Token using GitHub CLI's credential store when available.
/// On macOS, Keychain is used as a fallback.
pub fn save_token_persistently(token: &str) -> Result<String> {
    let mut github_cli_error = None;
    if command_exists("gh") {
        match save_token_with_github_cli(token) {
            Ok(()) => return Ok("GitHub CLI credential store".to_owned()),
            Err(error) => github_cli_error = Some(error),
        }
    }

    #[cfg(target_os = "macos")]
    {
        match save_token_to_macos_keychain(token) {
            Ok(()) => return Ok("macOS Keychain".to_owned()),
            Err(keychain_error) => {
                if let Some(cli_error) = github_cli_error {
                    return Err(anyhow!(
                        "GitHub CLI and macOS Keychain both rejected the token: {cli_error}; {keychain_error}"
                    ));
                }
                return Err(keychain_error);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(error) = github_cli_error {
            return Err(error);
        }
        Err(anyhow!(
            "Install GitHub CLI to retain a PAT securely on this platform, or use an environment variable"
        ))
    }
}

fn save_token_with_github_cli(token: &str) -> Result<()> {
    let mut child = Command::new("gh")
        .args(["auth", "login", "--hostname", "github.com", "--with-token"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not start GitHub CLI credential storage")?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(format!("{token}\n").as_bytes())
            .context("Could not send the token to GitHub CLI")?;
    }
    let output = child
        .wait_with_output()
        .context("Could not wait for GitHub CLI")?;
    if output.status.success() {
        Ok(())
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if message.is_empty() {
            Err(anyhow!("GitHub CLI could not save the token"))
        } else {
            Err(anyhow!("GitHub CLI could not save the token: {message}"))
        }
    }
}

#[must_use]
pub fn token_from_keychain(anonymous: bool) -> Option<String> {
    if anonymous {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("security")
            .args([
                "find-generic-password",
                "-a",
                KEYCHAIN_ACCOUNT,
                "-s",
                KEYCHAIN_SERVICE,
                "-w",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout)
            .ok()
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn save_token_to_macos_keychain(token: &str) -> Result<()> {
    let status = Command::new("security")
        .args([
            "add-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
            token,
            "-U",
        ])
        .status()
        .context("Could not access macOS Keychain")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("macOS Keychain rejected the token"))
    }
}

fn read_gh_token() -> Option<String> {
    let output = Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

fn ensure_gh_installed() -> Result<()> {
    if command_exists("gh") {
        Ok(())
    } else {
        Err(anyhow!(
            "GitHub CLI was not found. Install it from https://cli.github.com/ or choose PAT session mode"
        ))
    }
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
