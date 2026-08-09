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
A missing exact `owner/repo` gets a dedicated 404 message, with authentication or token-permission guidance for private repositories.

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
- Open, Closed, and All filters for both Pull requests and Issues
- Explicit `o` key for links that open GitHub in a browser
- Branch switching and recursive file finder
- Repository search, repository-wide code search, and current-file Find
- Blame, file history, commit details, and old/new diff line numbers
- Function/type/symbol outline, current-file-first definition search, live scan progress, cancellation, and result navigation
- Language-aware analysis and syntax colors for Rust, C/C++, C#, Java, Kotlin, Swift, Go, Python, Ruby, Shell, JavaScript/TypeScript, JSON, YAML, TOML, SQL, HTML/XML, CSS, Markdown, Lua, Haskell, Make, Dockerfile, CMake, Starlark, and fallback text
- Filename-first detection for `Makefile`, `Dockerfile.*`, `CMakeLists.txt`, `Gemfile`, `Jenkinsfile`, `BUILD`, and other extensionless files, followed by extension, shebang, and content heuristics
- Dark theme by default and a white-background Light theme
- Persistent source/diff wrapping settings
- Persistent GitHub REST/source cache with ETag validation, fetch timestamps, offline fallback, and cache manager
- Mouse-wheel navigation plus clickable lists, tabs, search results, code lines, and footer actions
- Keyboard line selection and system clipboard integration
- Print-oriented HTML export with line numbers, syntax colors, and readable A4 landscape CSS

</details>

<details>
<summary><strong>Keys</strong></summary>

```text
Home
  Enter              open exact owner/repo or search
  Up/Down, Tab       move through sections and items
  PageUp/PageDown    move by page
  Home/End           first/last item
  F5, Ctrl+R, r      force online ETag revalidation
  F8, c              cache manager
  Esc                clear input; quit when input is empty

Repository
  Left/Right, h/l, Tab  switch tabs
  1..6                 Code / Commits / PR / Issues / Actions / Releases
  Up/Down, PageUp/Down move by item/page
  Enter                open the selected item inside RepoTrek
  o                    open the selected GitHub link in a browser
  Esc, u, ..           parent directory/back
  B                    switch branch
  f                    recursive file finder
  s, /                 repository-wide source search; Esc/Ctrl+C cancels
  [ or ]                previous/next PR or Issue state filter
  O / C / A             show Open / Closed / All PRs or Issues
  F5, r                force-revalidate the current view

Readers
  Up/Down, PageUp/Down move by line/page
  Home/g, End/G        first/last line
  Left/Right, h/l      horizontal scroll
  Shift+J/K            extend line selection
  Ctrl+A / Shift+A     select all
  y / Ctrl+C           copy selection
  Esc                  clear selection before leaving
  v                    start/clear selection
  w                    toggle wrapping
  Tab                  Code / Blame / History in a source file
  / / Ctrl+F           Find in the current file
  n / N                next/previous match
  @                    symbols in the current file
  d                    definition near the cursor or by entered name; Esc/Ctrl+C cancels
  F5 / r               revalidate and reload the current file
  p                    export print-ready HTML

Palettes
  Up/Down, Tab         select a result
  PageUp/PageDown      move by page
  Home/End             first/last result
  Enter                search or open
  Ctrl+V               paste
  Esc                  close; also cancels an active repository search

Mouse
  Wheel                move/scroll
  Left click           select lists, tabs, results, code lines, or footer actions
  Click selected item  open/activate it
  Shift+click          extend source-line selection
  Right click          Esc equivalent

Global
  F2 / a               GitHub authentication
  ,                    settings
  T                    Dark / Light theme
  c / F8               cache manager
  ? / F1               help
  q / Ctrl+Q           quit
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
