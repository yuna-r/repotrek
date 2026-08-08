# RepoTrek

**A terminal-first GitHub source browser for reading code deeply without cloning a repository.**

[日本語](README.ja.md)

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

Open a repository directly with `repotrek owner/repo`.

<details>
<summary><strong>Features and keys</strong></summary>

RepoTrek brings GitHub-style repository browsing into a keyboard-driven TUI.

- Repository search from Home
- Code / Commits / Pull requests / Issues / Actions / Releases
- Branch switching
- Recursive file finder
- Repository-wide source search
- Line numbers and syntax highlighting
- Line-range selection and system clipboard copy
- Blame and file History
- Function/type/symbol jump
- Repository-wide definition/symbol search
- Commit diffs with old/new line numbers
- Print-friendly HTML export
- GitHub API token entry from the TUI
- GitHub CLI/browser authentication

```text
Home        Enter open/search       Ctrl+A auth
Repository  ←/→ tabs   u up   B branch   f files   s search
Source      Tab Code/Blame/History   v select   y copy
            @ symbols   d definition   p print
Global      ? help   q quit
```

</details>

<details>
<summary><strong>Build from source</strong></summary>

Requires Rust 1.97+. GitHub CLI (`gh`) is optional.

```bash
git clone https://github.com/yuna-r/repotrek.git
cd repotrek
cargo build --release
```

Binaries:

```text
target/release/repotrek       Linux / macOS
target/release/repotrek.exe   Windows
```

Development verification:

```bash
./scripts/verify.sh
```

On Windows:

```powershell
cargo fmt --all
cargo clippy --all-targets --all-features
cargo test --all-targets --all-features
cargo build --release
```

Linux clipboard integration uses `wl-copy`/`wl-paste`, `xclip`, or `xsel`.

</details>

<details>
<summary><strong>GitHub authentication</strong></summary>

Public repository browsing starts anonymously. GitHub currently limits unauthenticated REST requests to 60 per hour per originating IP, while normal authenticated user requests have a much larger primary quota. RepoTrek asks for authentication only when needed, or you can press `Ctrl+A`.

Authentication options:

1. GitHub CLI / browser
2. Personal Access Token for this session
3. Personal Access Token stored in macOS Keychain

Token entry is masked and supports `Ctrl+V`. Session PATs stay only in process memory. RepoTrek does not write PATs to its history database.

Environment token lookup order:

```text
REPOTREK_GITHUB_TOKEN
GH_TOKEN
GITHUB_TOKEN
```

Use `--anonymous` to ignore environment and Keychain tokens for that run. Blame uses GitHub GraphQL and therefore requires authentication.

</details>

<details>
<summary><strong>Printing</strong></summary>

Press `p` on a source file or commit. RepoTrek exports white-background HTML designed for reading and printing, including syntax colors, source line numbers, diff old/new line numbers, explicit `+`/`-` markers, and A4 landscape print CSS.

The temporary `Exported ...` status disappears automatically after a few seconds.

</details>

<details>
<summary><strong>Architecture</strong></summary>

```text
GitHub REST / GraphQL
        |
RepositoryProvider
        |
semantic models
        |
+----------------------------+
| TUI | search | print/export |
+----------------------------+
```

GitHub is the first provider. The provider boundary is separate so GitLab, Forgejo, Gitea, and generic Git backends can be added later.

</details>

## License

MIT License. See [LICENSE](LICENSE).
