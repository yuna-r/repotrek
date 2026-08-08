# Changelog

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
