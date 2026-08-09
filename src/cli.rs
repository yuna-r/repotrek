use clap::{Parser, Subcommand};

use crate::{icons::EmojiMode, theme::ThemeMode};

#[derive(Debug, Parser)]
#[command(
    name = "repotrek",
    version,
    about = "A terminal-first GitHub source browser & repository intelligence engine"
)]
pub struct Cli {
    /// owner/repo, a GitHub URL, or git@github.com:owner/repo.git
    pub repository: Option<String>,

    /// Intelligence subcommands
    #[command(subcommand)]
    pub command: Option<Commands>,

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

    /// Output report in JSON format
    #[arg(long)]
    pub json: bool,

    /// Output report in SARIF format
    #[arg(long)]
    pub sarif: bool,

    /// Output report in Markdown format
    #[arg(long)]
    pub markdown: bool,

    /// Output report in HTML format
    #[arg(long)]
    pub html: bool,

    /// Run analysis offline using local cached index
    #[arg(long)]
    pub offline: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run repository intelligence analysis
    Intelligence { path: Option<String> },
    /// Run architecture analysis
    Architecture { path: Option<String> },
    /// Run dependency graph & circular dependency scan
    Dependencies { path: Option<String> },
    /// Run security & secret scan
    Security { path: Option<String> },
    /// Run code quality & complexity analysis
    Quality { path: Option<String> },
    /// Display composite Repo Health Score (0-100)
    Health { path: Option<String> },
    /// Generate developer onboarding guide
    Onboard { path: Option<String> },
    /// Generate full executive report
    Report { path: Option<String> },
    /// Ask repository AI assistant
    Ai { query: String },
    /// Start Model Context Protocol (MCP) server
    Mcp,
}

