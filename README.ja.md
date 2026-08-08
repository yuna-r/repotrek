# RepoTrek

**GitHub上のソースコードを、cloneせずターミナルで深く読むためのTUIです。**

[English](README.md)

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

`repotrek owner/repo` でリポジトリを直接開けます。

<details>
<summary><strong>機能と操作</strong></summary>

- リポジトリ検索
- Code / Commits / Pull requests / Issues / Actions / Releases
- ブランチ切り替え
- ファイル検索・全文コード検索
- 行番号・構文ハイライト
- 行範囲選択とクリップボードコピー
- Blame / ファイルHistory
- 関数・型・シンボルジャンプ
- リポジトリ全体の定義検索
- old/new行番号付きコミットdiff
- 印刷向けHTMLエクスポート
- TUIからGitHub API Token入力

```text
Home        Enter open/search       Ctrl+A auth
Repository  ←/→ tabs   u up   B branch   f files   s search
Source      Tab Code/Blame/History   v select   y copy
            @ symbols   d definition   p print
Global      ? help   q quit
```

</details>

<details>
<summary><strong>ビルド</strong></summary>

```bash
git clone https://github.com/yuna-r/repotrek.git
cd repotrek
cargo build --release
```

</details>

<details>
<summary><strong>GitHub認証</strong></summary>

公開リポジトリは匿名APIから開始し、必要になった時点で認証できます。`Ctrl+A`から先に認証することもできます。PAT入力はマスクされ、セッションPATはプロセス内だけに保持されます。macOSではKeychain保存も選択できます。

</details>

<details>
<summary><strong>印刷</strong></summary>

ソースまたはコミットで`p`を押すと、行番号・構文色・diffのold/new行番号を含む白背景の印刷向けHTMLを書き出します。

</details>

## License

MIT License. [LICENSE](LICENSE) を参照してください。
