use clap::Parser;

use crate::{icons::EmojiMode, theme::ThemeMode};

#[derive(Debug, Parser)]
#[command(
    name = "repotrek",
    version,
    about = "A terminal-first GitHub source browser for reading repositories deeply"
)]
pub struct Cli {
    /// owner/repo, a GitHub URL, or git@github.com:owner/repo.git
    pub repository: Option<String>,

    /// Detect, force-enable, or disable emoji rendering
    #[arg(long, value_enum, default_value_t = EmojiMode::Auto)]
    pub emoji: EmojiMode,

    /// Override the saved dark or light theme for this run
    #[arg(long, value_enum)]
    pub theme: Option<ThemeMode>,

    /// Ignore saved and environment credentials and start anonymously
    #[arg(long)]
    pub anonymous: bool,

    /// Skip Featured and Recommended refresh at startup
    #[arg(long)]
    pub no_home_refresh: bool,
}
