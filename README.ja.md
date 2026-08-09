# RepoTrek

**GitHub上のリポジトリをcloneせず、ターミナルで深く読むためのソースブラウザです。**

[English](README.md)

> [!IMPORTANT]
> RepoTrekは匿名でも起動できますが、GitHub API認証を強く推奨します。匿名APIは制限が小さく、Blameなど認証必須の機能もあります。どこからでも`F2`、またはテキスト入力欄以外で`a`を押すと認証できます。GitHub CLIまたは永続保存した認証情報は、次回起動時にも再利用されます。

## 起動

1. **Linux**
   ```bash
   ./repotrek
   ```
2. **macOS**
   ```bash
   ./repotrek
   ```
3. **Windows**
   ```powershell
   .\repotrek.exe
   ```

`owner/repo`と完全に同じ形だけ、リポジトリを直接開きます。それ以外の入力はGitHubのbest-match検索を行い、RepoTrek側でも関連度を再評価して表示します。
存在しない`owner/repo`には専用の404表示を出し、非公開リポジトリの場合は認証またはToken権限の確認方法も案内します。

<details>
<summary><strong>認証</strong></summary>

`F2`を押すとどこからでも、またはテキスト入力欄以外で`a`を押すと、次の方式を選べます。

1. GitHub CLI／ブラウザ認証。GitHub CLIが永続保存
2. 現在のプロセスだけで使うPersonal Access Token
3. GitHub CLIまたはOSの資格情報ストアへ永続保存するPersonal Access Token

RepoTrek自身の履歴JSONや設定JSONにTokenを平文保存しません。`--anonymous`を付けると、その起動だけ保存済み認証と環境変数を無視します。

</details>

<details>
<summary><strong>主な機能</strong></summary>

- GitHubに近いCode、Commits、Pull requests、Issues、Actions、Releases
- PRの会話とdiff、Issueコメント、Actionsのjob/step、Release assetをTUI内で表示
- Pull requestsとIssuesの両方でOpen／Closed／Allを切替表示
- GitHubをブラウザで開く操作は`o`として明示
- ブランチ切替、再帰的ファイル検索、リポジトリ全文検索、現在ファイル内Find
- Blame、ファイルHistory、コミット詳細、old/new行番号付きdiff
- 関数・型・シンボル一覧、現在ファイル優先の定義位置検索、検索進捗、キャンセル、候補一覧からのジャンプ
- Rust、C/C++、C#、Java、Kotlin、Swift、Go、Python、Ruby、Shell、JavaScript/TypeScript、JSON、YAML、TOML、SQL、HTML/XML、CSS、Markdown、Lua、Haskell、Make、Dockerfile、CMake、Starlarkなどの言語別解析・構文色
- `Makefile`、`Dockerfile.*`、`CMakeLists.txt`、`Gemfile`、`Jenkinsfile`、`BUILD`などをファイル名で判定し、拡張子・shebang・内容推定へフォールバック
- ダークテーマを既定にし、白背景・黒文字を基本にしたライトテーマも搭載
- ソースとdiffの折返し設定を永続保存
- ETag再検証・取得時刻・オフラインフォールバック・管理画面を備えた永続キャッシュ
- マウスホイール移動、一覧・タブ・検索候補・コード行・下部アクションのクリック操作
- 行範囲選択とシステムクリップボードへのコピー
- 行番号・構文色・A4横向けCSSを備えた印刷用HTML

</details>

<details>
<summary><strong>主な操作</strong></summary>

```text
Home
  Enter              owner/repoを直接開く、または検索
  上下 / Tab          セクションと項目を移動
  PageUp/PageDown    一ページ移動
  Home/End           先頭／末尾
  F5 / Ctrl+R / r    ETagで強制再検証
  F8 / c             キャッシュ管理
  Esc                入力消去。空欄なら終了

Repository
  左右 / h/l / Tab   タブ切替
  1..6               Code / Commits / PR / Issues / Actions / Releases
  上下・PageUp/Down  項目／ページ移動
  Enter              選択項目をRepoTrek内で開く
  o                  GitHubへの外部リンクを開く
  Esc / u / ..       上のディレクトリ／戻る
  B                  ブランチ切替
  f                  再帰的ファイル検索
  s /                リポジトリ全文検索。Esc／Ctrl+Cでキャンセル
  [ または ]          PR・Issueの状態を前／次へ切替
  O / C / A           Open／Closed／Allを直接選択
  F5 / r             現在画面を強制再検証

Reader
  上下・PageUp/Down  行／ページ移動
  Home/g・End/G      先頭／末尾
  左右 / h/l         横スクロール
  Shift+J/K          行範囲選択
  Ctrl+A / Shift+A   全選択
  y / Ctrl+C         コピー
  Esc                選択解除（未選択時は戻る）
  v                  選択開始／解除
  w                  折返し切替
  Tab                Code / Blame / History
  / / Ctrl+F         現在ファイル内Find
  n / N              次／前の一致
  @                  現在ファイルのシンボル
  d                  カーソル付近または入力した名前の定義検索。Esc／Ctrl+Cでキャンセル
  F5 / r             現在ファイルを再検証して再取得
  p                  印刷用HTML

Palette
  上下 / Tab          候補移動
  PageUp/PageDown    一ページ移動
  Home/End           先頭／末尾
  Enter              検索または決定
  Ctrl+V             貼り付け
  Esc                閉じる。実行中のリポジトリ検索もキャンセル

Mouse
  ホイール           移動・スクロール
  左クリック         一覧、タブ、候補、コード行、下部キーを選択
  選択済み項目を再クリック  開く／決定
  Shift+クリック     コード行の範囲選択
  右クリック         Esc相当

Global
  F2 / a             GitHub認証
  ,                  設定
  T                  ダーク／ライト切替
  c / F8             キャッシュ管理
  ? / F1             ヘルプ
  q / Ctrl+Q         終了
```

</details>

<details>
<summary><strong>ソースからビルド</strong></summary>

```bash
git clone https://github.com/yuna-r/repotrek.git
cd repotrek
cargo build --release
```

```bash
./scripts/verify.sh
```

</details>

<details>
<summary><strong>配布バイナリ</strong></summary>

`v*`タグをpushすると、Linux x86_64／ARM64、macOS Apple Silicon／Intel、Windows x86_64／ARM64を自動ビルドし、GitHub Releasesへ添付します。

</details>

## License

MIT Licenseです。[LICENSE](LICENSE)を参照してください。
