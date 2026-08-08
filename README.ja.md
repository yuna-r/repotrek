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
- GitHubをブラウザで開く操作は`o`として明示
- ブランチ切替、再帰的ファイル検索、全文コード検索
- Blame、ファイルHistory、コミット詳細、old/new行番号付きdiff
- 関数・型・シンボル一覧、定義位置検索
- Rust、C/C++、C#、Java、Kotlin、Swift、Go、Python、Ruby、Shell、JavaScript/TypeScript、JSON、YAML、TOML、SQL、HTML/XML、CSS、Markdown、Lua、Haskellなどの言語別構文色
- ダークテーマを既定にし、白背景・黒文字を基本にしたライトテーマも搭載
- ソースとdiffの折返し設定を永続保存
- 行範囲選択とシステムクリップボードへのコピー
- 行番号・構文色・A4横向けCSSを備えた印刷用HTML

</details>

<details>
<summary><strong>主な操作</strong></summary>

```text
Home
  Enter             owner/repoを直接開く、または検索
  上下 / Tab         セクションと項目を一貫して移動
  F5 / Ctrl+R        更新
  Esc               入力消去。空欄なら終了

Repository
  左右 / h/l         タブ切替
  1..6               Code / Commits / PR / Issues / Actions / Releases
  Enter              選択項目をRepoTrek内で開く
  o                  GitHubへの外部リンクを開く
  Esc / u / ..       上のディレクトリ
  B                  ブランチ切替
  f                  ファイル検索
  s /                リポジトリ全文検索

Reader
  上下               1行移動
  Shift+J/K          行範囲選択
  Shift+A            全選択
  Shift+C            コピー
  Esc                 選択解除（未選択時は戻る）
  v / y              Vim風の選択／コピー
  w                  折返し切替
  Tab                Code / Blame / History
  @                  現在ファイルのシンボル
  d                  定義・シンボル検索
  p                  印刷用HTML

Global
  F2 / a             GitHub認証
  ,                  設定
  T                  ダーク／ライト切替
  ?                  ヘルプ
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
