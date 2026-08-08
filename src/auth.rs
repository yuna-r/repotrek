use std::{env, process::Command};

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

pub fn authenticate_with_github_cli() -> Result<String> {
    if let Some(token) = read_gh_token() {
        return Ok(token);
    }

    let installed = Command::new("gh")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !installed {
        return Err(anyhow!(
            "GitHub CLI was not found. On macOS: brew install gh"
        ));
    }

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

pub fn save_token_to_keychain(token: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
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
    #[cfg(not(target_os = "macos"))]
    {
        let _ = token;
        Err(anyhow!(
            "Persistent PAT storage is currently implemented for macOS Keychain only; use session mode or an environment variable on this platform"
        ))
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
