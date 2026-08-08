# RepoTrek

A terminal-first source code browser for GitHub repositories.

RepoTrekは、Git操作ではなく**ソースコードを読むこと**を主目的にしたTUIです。GitHubのRepository、Code、Commits、Commit detailという情報設計をターミナルへ移し、履歴を辿りながらコードを読む速度を高めます。

## 現在のMVP

```text
 RepoTrek  Terminal-first source code browser

 🔎 Repository
 ┌──────────────────────────────────────────────────────────────┐
 │ > owner/repo, GitHub URL, or git@github.com URL             │
 └──────────────────────────────────────────────────────────────┘

 🕘 History             ✨ Featured             🧭 Recommended
 ┌───────────────────┐  ┌───────────────────┐  ┌───────────────────┐
 │ rust-lang/rust    │  │ torvalds/linux    │  │ ratatui/ratatui  │
 │ src/lib.rs        │  │ C · ★120k         │  │ Rust · ★120k     │
 └───────────────────┘  └───────────────────┘  └───────────────────┘
```

MVPには次の機能が入っています。

- `owner/repo`、GitHub URL、GitHub SSH URLから公開リポジトリを開く
- GitHubライクなCodeツリーを移動し、UTF-8テキストファイルを読む
- 行番号、縦横スクロール、軽量なシンタックスハイライト
- Commits一覧、コミット概要、変更ファイル、patchの閲覧
- Historyへ最後に読んでいたファイルやコミットを保存し、次回復帰
- GitHub SearchによるFeaturedと、閲覧言語を使ったRecommended
- 絵文字表示の`auto`、`on`、`off`
- ファイルとコミットdiffをA4印刷向けHTMLへ書き出す
- Provider境界を分離し、GitLab、Gitea、Forgejo、汎用Gitを追加できる構造

Pull requests、Issues、Actions、Releases、Blame、Historyタブは情報設計だけ先に表示しており、MVPでは未実装です。

## macOSで起動

Apple SiliconとIntel Macのどちらでも同じ手順です。

```bash
xcode-select --install
brew install rustup gh

printf '\nexport PATH="$(brew --prefix rustup)/bin:$PATH"\n' >> ~/.zshrc
source ~/.zshrc

cd repotrek
rustup show
cargo run --release
```

`rust-toolchain.toml`がRust 1.97.1、rustfmt、Clippyを自動選択します。

直接リポジトリを開く場合です。

```bash
cargo run --release -- torvalds/linux
cargo run --release -- rust-lang/rust
cargo run --release -- https://github.com/yuna-r/repotrek
cargo run --release -- git@github.com:yuna-r/repotrek.git
```

インストールする場合です。

```bash
cargo install --path .
repotrek
```

RepoTrekはバイナリアプリケーションなので、初回ビルドで生成された`Cargo.lock`はコミットしてください。

## 操作

### Home

| Key | Action |
|---|---|
| `Enter` | 入力したリポジトリ、または選択中のカードを開く |
| `Tab` / `Shift+Tab` | Search、History、Featured、Recommendedを移動 |
| `j` / `k` | カードを移動 |
| `/` | Repository入力へ移動 |
| `r` | FeaturedとRecommendedを更新 |
| `q` | 終了 |

### Repository

| Key | Action |
|---|---|
| `1` / `2` | Code / Commits |
| `j` / `k` | ファイルまたはコミットを移動 |
| `Enter` | ディレクトリ、ファイル、コミットを開く |
| `Backspace` | 親ディレクトリへ移動 |
| `n` / `p` | コミットの次ページ / 前ページ |
| `Esc` | Homeへ戻る |

### File / Commit

| Key | Action |
|---|---|
| `j` / `k` | 縦スクロール |
| `h` / `l` | ファイルを横スクロール |
| `PageUp` / `PageDown` | ページ単位で移動 |
| `g` / `G` | 先頭 / 末尾 |
| `p` | 印刷用HTMLを書き出す |
| `b` | Repositoryへ戻る |

どの画面でも`?`でヘルプを表示します。入力欄にフォーカスがある間の`?`は文字として入力されます。

## GitHub APIと認証

初回起動時はGitHubアカウントを要求せず、匿名REST APIでそのまま閲覧します。コアAPIの匿名上限を使い切ったときだけ認証ダイアログを表示します。

`Enter`を押すとGitHub CLIのWeb認証を開始します。TokenはGitHub CLIが保管し、RepoTrekは現在のプロセスのメモリ内でのみ利用します。

先にTokenを渡す場合は次のどちらかを使えます。

```bash
export REPOTREK_GITHUB_TOKEN="your-token"
export GH_TOKEN="your-token"
export GITHUB_TOKEN="your-token"
```

Tokenを無視して匿名動作を確認する場合です。

```bash
repotrek --anonymous
```

起動時のGitHub Searchを行わず、内蔵カードとローカル履歴だけでHomeを開く場合です。

```bash
repotrek --no-home-refresh
```

## 絵文字

```bash
repotrek --emoji auto
repotrek --emoji on
repotrek --emoji off
```

`auto`はUTF-8 locale、`TERM`、`NO_EMOJI`を使って保守的に判定します。端末からフォントのグリフ有無を完全には取得できないため、表示が合わない場合は`on`または`off`で固定してください。

## 印刷

ファイルまたはコミット画面で`p`を押すと、現在のディレクトリへ次の形式でHTMLを書き出します。

```text
repotrek-export-owner-repo-src-main.rs.html
repotrek-export-owner-repo-commit-abcdef0.html
```

HTMLには次を含みます。

- RepositoryとCommitのGitHubリンク
- A4向け`@page`設定
- 行番号付きコード
- 色だけに依存しない`+`と`−`のdiff表現
- 白背景、高コントラスト、印刷時の改ページ制御

macOSでは次のように開いて、そのまま印刷またはPDF保存できます。

```bash
open repotrek-export-owner-repo-src-main.rs.html
```

## データ保存

履歴は`directories` crateが返すOS標準のApplication Support領域へJSONで保存します。macOSでは通常、次の配下です。

```text
~/Library/Application Support/dev.yuna-r.RepoTrek/history.json
```

壊れたJSONを検出した場合は`.json.corrupt`へ退避し、新しい履歴から再開します。

## 構造

```text
src/
├── app.rs              画面状態とキー操作
├── auth.rs             GitHub CLI認証
├── export.rs           印刷用HTML
├── highlight.rs        軽量ハイライト
├── icons.rs            絵文字とASCIIフォールバック
├── model.rs            Provider非依存モデル
├── provider/
│   ├── mod.rs          RepositoryProvider trait
│   └── github.rs       GitHub REST API実装
├── storage.rs          History永続化
├── ui.rs               Ratatuiレンダラー
└── main.rs             イベントループとコマンド実行
```

ネットワーク処理中はLoading表示を描画してから同期リクエストを実行します。初版では状態遷移を単純に保ち、次段階でworker threadとmessage queueへ置換できる境界にしています。

## 開発時の検証

```bash
./scripts/verify.sh
```

個別に実行する場合です。

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features
cargo test --all-targets --all-features
cargo build --release
```

GitHub ActionsではmacOSとUbuntuの両方で同じ検証を行います。

## License

MIT License
