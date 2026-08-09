# Changelog

## 0.4.0 - 2026-08-09

- Added a persistent GitHub REST/source cache with a 15-minute freshness window.
- Added conditional ETag/Last-Modified validation and stale-cache fallback for network and GitHub 5xx failures.
- Cache metadata records first fetch, content fetch, validation, access, size, and hit count.
- Added `F8`/`c` cache manager with summary and confirmed full-cache deletion.
- `F5`, `Ctrl+R`, and repository `r` now force online revalidation.
- Reworked the footer into a dynamically wrapped, fully visible, clickable action/status area.
- Wrapped the left header title while preserving the top-right authentication/rate-limit area.
- Increased Home card-title contrast for History, Featured, and Recommended.
- Added mouse-wheel navigation and clickable lists, tabs, palette results, source lines, and footer actions.
- Added current-file Find with `/`/`Ctrl+F`, live matches, `n`/`N`, and page navigation.
- Reworked repository search, file search, code search, Symbols, and Definition palettes so typed `j`/`k` are not stolen as navigation keys.
- Added PageUp/PageDown, Home/End, explicit empty-result messages, and clickable selection to palettes.
- Expanded symbol/definition detection across major languages plus Make, Dockerfile, CMake, Starlark, and common configuration formats.
- Added filename-first language detection for `Makefile`, `Dockerfile.*`, `CMakeLists.txt`, `Gemfile`, `Jenkinsfile`, `BUILD`, and other extensionless files, followed by extension, shebang, and content heuristics.
- Added dedicated 404 messages for missing `owner/repository`, files, directories, and branches, including private-repository authentication guidance.
- Made repository-wide code and Definition searches cancellable with `Esc`/`Ctrl+C`, live file-by-file progress, and bounded Definition scanning.
- Added Open/Closed/All filters for both Pull requests and Issues, with `[`/`]` cycling and direct `O`/`C`/`A` shortcuts.

## 0.3.9

- Fixed strict Clippy checks used by the crates.io publishing workflow.
- Scoped macOS Keychain constants to macOS builds.
- Cleaned up Clippy-reported control flow and highlighting code.


## 0.3.5

- Made `Shift+A` the official select-all shortcut for readers.
- Made `Shift+C` the official copy-selection shortcut for readers.
- `Esc` now clears an active selection before navigating away.
- Kept `Ctrl+A` / `Ctrl+C` as compatibility shortcuts where terminals deliver them.

## 0.3.4

- Made `Shift+J` / `Shift+K` the official line-range selection shortcuts.
- Removed documented reliance on `Shift+Up` / `Shift+Down`, which is intercepted or normalized by some macOS terminal environments.
- Kept the improved Dark and Light current-line highlighting from v0.3.3.

## 0.3.3

- Changed line-range selection from `Ctrl+Up` / `Ctrl+Down` to `Shift+Up` / `Shift+Down` to avoid macOS and terminal shortcut conflicts.
- Updated in-app keyboard help and README documentation for the new selection shortcut.

## 0.3.2

- Add per-entry browsing history deletion from the Home screen.
- Add confirmed clearing of all local browsing history.

## 0.3.0 - 2026-08-08

- Added persistent GitHub authentication and in-app PAT entry.
- Added internal TUI views for pull requests, issues, Actions runs, and releases.
- Added consistent Home navigation, exact `owner/repo` opening, best-match repository search, and local result reranking.
- Added `Esc`, `u`, and `..` parent-directory navigation.
- Added line selection with `Ctrl+Up` / `Ctrl+Down`, `Ctrl+A`, and `Ctrl+C`.
- Added persistent source and diff wrapping preferences.
- Added language-aware source and diff highlighting.
- Added Dark and Light themes with persistent settings.
- Added repository file search, code search, Blame, file history, symbols, and definition search.
- Improved print-oriented HTML export and transient status messages.
- Added six-platform GitHub Release builds for Linux, macOS, and Windows on x86_64 and ARM64.
