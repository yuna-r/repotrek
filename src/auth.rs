use std::{env, process::Command};

use anyhow::{Context, Result, anyhow};

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
            "GitHub CLIが見つかりません。Macでは `brew install gh` の後に再試行してください"
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
        .context("GitHub CLIを起動できません")?;

    if !status.success() {
        return Err(anyhow!("GitHub認証が完了しませんでした"));
    }

    read_gh_token().ok_or_else(|| anyhow!("GitHub CLIからアクセストークンを取得できません"))
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
