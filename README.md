# RepoTrek

**A terminal-first GitHub source browser for reading repositories deeply without cloning them.**

[日本語](README.ja.md)

> [!IMPORTANT]
> RepoTrek can start anonymously, but GitHub API authentication is strongly recommended. Anonymous access is heavily rate-limited, and features such as Blame require authentication. Press `F2` anywhere, or `a` outside a text field, to sign in. GitHub CLI and persistent token credentials are reused on later launches.

## Run

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

Enter exactly `owner/repo` to open a repository directly. Every other value uses GitHub repository search with best-match results and RepoTrek relevance reranking.

<details>
<summary><strong>Authentication</strong></summary>

Press `F2` anywhere, or `a` outside a text field, to choose one of these methods:

1. GitHub CLI / browser sign-in, persisted by GitHub CLI
2. Personal Access Token for the current process only
3. Personal Access Token persisted through GitHub CLI or the operating-system credential store

RepoTrek checks credentials in this order:

```text
REPOTREK_GITHUB_TOKEN
GH_TOKEN
GITHUB_TOKEN
GitHub CLI credentials
OS credential store fallback
anonymous access
```

Tokens are never written to RepoTrek's history or settings JSON files. Use `--anonymous` to ignore stored and environment credentials for one run.

</details>

<details>
<summary><strong>Main features</strong></summary>

- GitHub-style Code, Commits, Pull requests, Issues, Actions, and Releases tabs
- Pull request conversation/diff, issue comments, workflow jobs/steps, and release assets inside the TUI
- Explicit `o` key for links that open GitHub in a browser
- Branch switching and recursive file finder
- Repository search and repository-wide code search
- Blame, file history, commit details, and old/new diff line numbers
- Function/type/symbol list and definition search
- Language-aware syntax colors for Rust, C/C++, C#, Java, Kotlin, Swift, Go, Python, Ruby, Shell, JavaScript/TypeScript, JSON, YAML, TOML, SQL, HTML/XML, CSS, Markdown, Lua, Haskell, and fallback text
- Dark theme by default and a white-background Light theme
- Persistent source/diff wrapping settings
- Keyboard line selection and system clipboard integration
- Print-oriented HTML export with line numbers, syntax colors, and readable A4 landscape CSS

</details>

<details>
<summary><strong>Keys</strong></summary>

```text
Home
  Enter             open exact owner/repo or search
  Up/Down, Tab      move through sections consistently
  F5, Ctrl+R        refresh
  Esc               clear input; quit when input is empty

Repository
  Left/Right, h/l   switch tabs
  1..6              Code / Commits / PR / Issues / Actions / Releases
  Enter             open the selected item inside RepoTrek
  o                 open the selected GitHub link in a browser
  Esc, u, ..        parent directory
  B                 switch branch
  f                 recursive file finder
  s, /              repository-wide source search

Readers
  Up/Down            move by line
  Shift+J/K          extend line selection
  Shift+A            select all
  Shift+C            copy selection
  Esc                 clear selection before leaving
  v, y                Vim-style select/copy alternatives
  w                  toggle wrapping
  Tab                Code / Blame / History in a source file
  @                  symbols in the current file
  d                  definition/symbol search
  p                  export print-ready HTML

Global
  F2 / a             GitHub authentication
  ,                  settings
  T                  Dark / Light theme
  ?                  help
  q, Ctrl+Q          quit
```

</details>

<details>
<summary><strong>Build from source</strong></summary>

Rust 1.97 or later is required.

```bash
git clone https://github.com/yuna-r/repotrek.git
cd repotrek
cargo build --release
```

Development verification:

```bash
./scripts/verify.sh
```

Linux clipboard integration uses `wl-copy`/`wl-paste`, `xclip`, or `xsel` when available.

</details>

<details>
<summary><strong>Release binaries</strong></summary>

Pushing a signed `v*` tag runs the release workflow and attaches these archives to GitHub Releases:

```text
repotrek-linux-x86_64.tar.gz
repotrek-linux-aarch64.tar.gz
repotrek-macos-aarch64.tar.gz
repotrek-macos-x86_64.tar.gz
repotrek-windows-x86_64.zip
repotrek-windows-aarch64.zip
SHA256SUMS
```

</details>

## License

MIT License. See [LICENSE](LICENSE).
