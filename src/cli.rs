use clap::Parser;

use crate::icons::EmojiMode;

#[derive(Debug, Parser)]
#[command(
    name = "repotrek",
    version,
    about = "A terminal-first source code browser for GitHub repositories"
)]
pub struct Cli {
    /// owner/repo、GitHub URL、またはgit@github.com形式
    pub repository: Option<String>,

    /// 絵文字表示を自動判定・強制有効・無効にします
    #[arg(long, value_enum, default_value_t = EmojiMode::Auto)]
    pub emoji: EmojiMode,

    /// 環境変数のGitHub tokenも使わず、必ず匿名で開始します
    #[arg(long)]
    pub anonymous: bool,

    /// 起動時のFeatured／Recommended更新を省略します
    #[arg(long)]
    pub no_home_refresh: bool,
}
