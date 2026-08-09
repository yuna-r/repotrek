use std::{
    io::Write,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow};

pub fn copy_text(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        pipe_to("pbcopy", &[], text)
    }
    #[cfg(target_os = "windows")]
    {
        return pipe_to(
            "powershell.exe",
            &["-NoProfile", "-Command", "Set-Clipboard"],
            text,
        );
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if command_exists("wl-copy") {
            return pipe_to("wl-copy", &[], text);
        }
        if command_exists("xclip") {
            return pipe_to("xclip", &["-selection", "clipboard"], text);
        }
        if command_exists("xsel") {
            return pipe_to("xsel", &["--clipboard", "--input"], text);
        }
        Err(anyhow!(
            "No clipboard helper found. Install wl-clipboard, xclip, or xsel"
        ))
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = text;
        Err(anyhow!("Clipboard is not supported on this platform"))
    }
}

pub fn paste_text() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        read_from("pbpaste", &[])
    }
    #[cfg(target_os = "windows")]
    {
        return read_from(
            "powershell.exe",
            &["-NoProfile", "-Command", "Get-Clipboard -Raw"],
        );
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if command_exists("wl-paste") {
            return read_from("wl-paste", &["--no-newline"]);
        }
        if command_exists("xclip") {
            return read_from("xclip", &["-selection", "clipboard", "-o"]);
        }
        if command_exists("xsel") {
            return read_from("xsel", &["--clipboard", "--output"]);
        }
        Err(anyhow!(
            "No clipboard helper found. Install wl-clipboard, xclip, or xsel"
        ))
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        Err(anyhow!("Clipboard is not supported on this platform"))
    }
}

fn pipe_to(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("Could not start {program}"))?;
    child
        .stdin
        .as_mut()
        .context("Clipboard process stdin is unavailable")?
        .write_all(text.as_bytes())?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{program} returned a non-zero status"))
    }
}

fn read_from(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("Could not start {program}"))?;
    if !output.status.success() {
        return Err(anyhow!("{program} returned a non-zero status"));
    }
    String::from_utf8(output.stdout).context("Clipboard did not contain UTF-8 text")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn command_exists(program: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {program} >/dev/null 2>&1")])
        .status()
        .is_ok_and(|status| status.success())
}
