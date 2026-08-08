use std::env;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EmojiMode {
    Auto,
    On,
    Off,
}

impl EmojiMode {
    #[must_use]
    pub fn resolve(self) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Auto => terminal_likely_supports_emoji(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Icons {
    pub enabled: bool,
    pub search: &'static str,
    pub history: &'static str,
    pub featured: &'static str,
    pub recommended: &'static str,
    pub folder: &'static str,
    pub file: &'static str,
    pub branch: &'static str,
    pub commit: &'static str,
    pub verified: &'static str,
    pub warning: &'static str,
    pub star: &'static str,
    pub fork: &'static str,
    pub print: &'static str,
}

impl Icons {
    #[must_use]
    pub fn new(mode: EmojiMode) -> Self {
        let enabled = mode.resolve();
        if enabled {
            Self {
                enabled,
                search: "🔎",
                history: "🕘",
                featured: "✨",
                recommended: "🧭",
                folder: "📁",
                file: "📄",
                branch: "🌿",
                commit: "●",
                verified: "✅",
                warning: "⚠",
                star: "★",
                fork: "⑂",
                print: "🖨",
            }
        } else {
            Self {
                enabled,
                search: "",
                history: "",
                featured: "",
                recommended: "",
                folder: "",
                file: "",
                branch: "",
                commit: "*",
                verified: "[verified]",
                warning: "[!]",
                star: "*",
                fork: "fork",
                print: "",
            }
        }
    }

    #[must_use]
    pub fn label(&self, icon: &str, text: &str) -> String {
        if self.enabled && !icon.is_empty() {
            format!("{icon} {text}")
        } else {
            text.to_owned()
        }
    }
}

fn terminal_likely_supports_emoji() -> bool {
    if env::var_os("NO_EMOJI").is_some() {
        return false;
    }
    if env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb")) {
        return false;
    }

    let locale = env::var("LC_ALL")
        .or_else(|_| env::var("LC_CTYPE"))
        .or_else(|_| env::var("LANG"))
        .unwrap_or_default()
        .to_ascii_uppercase();
    locale.contains("UTF-8") || locale.contains("UTF8")
}
