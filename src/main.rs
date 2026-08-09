mod app;
mod auth;
mod cache;
mod cli;
mod clipboard;
mod diff;
mod export;
mod highlight;
mod icons;
mod language;
mod model;
mod provider;
mod settings;
mod storage;
mod symbols;
mod theme;
mod ui;

use std::{
    collections::HashSet,
    io::stdout,
    process::Command,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TryRecvError},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use clap::Parser;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
        KeyboardEnhancementFlags, MouseButton, MouseEventKind, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::supports_keyboard_enhancement,
};
use ratatui::DefaultTerminal;

use crate::{
    app::{App, AppCommand, CodeSearchMode, DetailDocument, RepositoryTab},
    cli::Cli,
    icons::Icons,
    model::{ApiResponse, CodeSearchResult, HistoryScreen, RateLimit, RepositoryId, TreeEntry},
    provider::{ProviderError, ProviderResult, RepositoryProvider, github::GitHubProvider},
    settings::{SettingsStore, save_settings},
    storage::HistoryStore,
};

enum CodeSearchWorkerMessage {
    Progress(String),
    Finished(ProviderResult<Vec<CodeSearchResult>>),
}

struct CancelSearchOnDrop(Arc<AtomicBool>);

impl Drop for CancelSearchOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn main() -> Result<()> {
    install_panic_hook();
    let cli = Cli::parse();

    let settings_store = SettingsStore::load()?;
    let mut settings = settings_store.settings().clone();
    if let Some(theme) = cli.theme {
        settings.theme = theme;
    }

    let token = auth::token_from_environment(cli.anonymous)
        .or_else(|| auth::token_from_github_cli(cli.anonymous))
        .or_else(|| auth::token_from_keychain(cli.anonymous));
    let mut provider = GitHubProvider::new(token)?;
    let mut history = HistoryStore::load()?;
    let icons = Icons::new(cli.emoji);
    let mut app = App::new(
        history.entries().to_vec(),
        icons,
        provider.is_authenticated(),
        settings,
    );

    let mut terminal = ratatui::try_init().context("Could not initialize terminal")?;
    let _ = enable_mouse_capture();
    let keyboard_enhancement = enable_keyboard_enhancement();
    let result = run(
        &mut terminal,
        &mut app,
        &mut provider,
        &mut history,
        cli.repository,
        cli.no_home_refresh,
    );
    disable_mouse_capture();
    if keyboard_enhancement {
        disable_keyboard_enhancement();
    }
    let restore_result = ratatui::try_restore().context("Could not restore terminal");

    match (result, restore_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn enable_keyboard_enhancement() -> bool {
    if !matches!(supports_keyboard_enhancement(), Ok(true)) {
        return false;
    }

    let mut output = stdout();
    execute!(
        output,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
    .is_ok()
}

fn disable_keyboard_enhancement() {
    let mut output = stdout();
    let _ = execute!(output, PopKeyboardEnhancementFlags);
}

fn enable_mouse_capture() -> bool {
    let mut output = stdout();
    execute!(output, EnableMouseCapture).is_ok()
}

fn disable_mouse_capture() {
    let mut output = stdout();
    let _ = execute!(output, DisableMouseCapture);
}

fn run(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    provider: &mut GitHubProvider,
    history: &mut HistoryStore,
    initial_repository: Option<String>,
    no_home_refresh: bool,
) -> Result<()> {
    terminal.draw(|frame| ui::draw(frame, app))?;

    if let Some(repository) = initial_repository {
        match RepositoryId::from_str(&repository) {
            Ok(id) => execute_command(
                AppCommand::OpenRepository {
                    id,
                    resume_path: None,
                    resume_screen: HistoryScreen::Code,
                },
                app,
                provider,
                history,
                terminal,
            )?,
            Err(error) => app.show_error("Repository", error.to_string()),
        }
    } else if !no_home_refresh {
        execute_command(AppCommand::RefreshHome, app, provider, history, terminal)?;
    }

    loop {
        app.expire_status();
        terminal.draw(|frame| ui::draw(frame, app))?;
        if !event::poll(Duration::from_millis(120))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if !key.is_release() => {
                let command = app.handle_key(key);
                if matches!(command, AppCommand::Quit) {
                    break;
                }
                execute_command(command, app, provider, history, terminal)?;
            }
            Event::Mouse(mouse) => {
                let (width, height) = crossterm::terminal::size()?;
                let command = ui::handle_mouse(app, mouse, width, height);
                if matches!(command, AppCommand::Quit) {
                    break;
                }
                execute_command(command, app, provider, history, terminal)?;
            }
            Event::Paste(text) => app.handle_paste(text),
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

fn execute_command(
    command: AppCommand,
    app: &mut App,
    provider: &mut GitHubProvider,
    history: &mut HistoryStore,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    match command.clone() {
        AppCommand::None | AppCommand::Quit => {}
        AppCommand::ForceRefresh(inner) => {
            provider.set_force_refresh(true);
            let result = execute_command(*inner, app, provider, history, terminal);
            provider.set_force_refresh(false);
            result?;
        }
        AppCommand::ShowCacheManager => {
            app.show_cache_manager(provider.cache_summary().lines());
        }
        AppCommand::ClearCache => match provider.clear_cache() {
            Ok(cleared) => {
                app.modal = None;
                app.set_cache_status(None);
                app.set_status(format!(
                    "Cleared {} cached responses ({} bytes)",
                    cleared.entries, cleared.bytes
                ));
            }
            Err(error) => app.show_error("Cache", error.to_string()),
        },
        AppCommand::OpenRepository {
            id,
            resume_path,
            resume_screen,
        } => open_repository(
            id,
            resume_path,
            resume_screen,
            command,
            app,
            provider,
            history,
            terminal,
        )?,
        AppCommand::RefreshHome => refresh_home(command, app, provider, history, terminal)?,
        AppCommand::DeleteHistory(id) => {
            if history.remove_repository(&id)? {
                app.update_history(history.entries().to_vec());
                app.set_status(format!("Removed {} from History", id.full_name()));
            }
        }
        AppCommand::ClearHistory => {
            history.clear()?;
            app.update_history(Vec::new());
            app.set_status("Browsing history cleared");
        }
        AppCommand::SearchRepositories(query) => {
            let results = request(
                app,
                terminal,
                format!("Searching repositories: {query}"),
                command,
                || provider.search_repositories(&smart_repository_query(&query), "best-match", 50),
            )?;
            if let Some(results) = results {
                app.set_repository_search_results(query, results);
            }
        }
        AppCommand::OpenDirectory(path) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.selected_ref.clone();
            let entries = request_with_context(
                app,
                terminal,
                format!("Loading {}/{}", id, display_path(&path)),
                command,
                RequestContext::Directory(path.clone()),
                || provider.contents(&id, &path, &git_ref),
            )?;
            if let Some(entries) = entries {
                app.set_directory(path.clone(), entries);
                update_history_location(app, history, &id, Some(path), HistoryScreen::Code);
            }
        }
        AppCommand::OpenFile {
            path,
            find,
            line,
            definition,
        } => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.selected_ref.clone();
            let content = request_with_context(
                app,
                terminal,
                format!("Loading {path}"),
                command,
                RequestContext::File(path.clone()),
                || provider.file_content(&id, &path, &git_ref),
            )?;
            if let Some(content) = content {
                let definition_line = if definition {
                    find.as_deref().and_then(|query| {
                        symbols::find_definition(&path, &content, query).map(|symbol| symbol.line)
                    })
                } else {
                    None
                };
                let target_line = definition_line.or(line);
                app.open_file(path.clone(), content, find.as_deref(), target_line);
                if definition && definition_line.is_none() {
                    app.set_status(format!(
                        "No declaration pattern found in {path}; jumped to the first text match"
                    ));
                }
                update_history_location(app, history, &id, Some(path), HistoryScreen::File);
            }
        }
        AppCommand::LoadRepositoryTab(tab) => {
            load_repository_tab(tab, command, app, provider, terminal)?;
        }
        AppCommand::LoadCommits { page } => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.selected_ref.clone();
            let commits = request(
                app,
                terminal,
                format!("Loading commits page {page}"),
                command,
                || provider.commits(&id, &git_ref, page, 50),
            )?;
            if let Some(commits) = commits {
                if commits.is_empty() && page > 1 {
                    app.set_status("No more commits");
                } else {
                    app.set_commits(page, commits);
                    update_history_location(app, history, &id, None, HistoryScreen::Commits);
                }
            }
        }
        AppCommand::OpenCommit(sha) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let detail = request(
                app,
                terminal,
                format!("Loading commit {}", short_sha(&sha)),
                command,
                || provider.commit(&id, &sha),
            )?;
            if let Some(detail) = detail {
                app.open_commit(detail);
                update_history_location(app, history, &id, Some(sha), HistoryScreen::Commit);
            }
        }
        AppCommand::OpenPullRequest(number) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let detail = request(
                app,
                terminal,
                format!("Loading pull request #{number}"),
                command,
                || provider.pull_request(&id, number),
            )?;
            if let Some(detail) = detail {
                app.open_detail(DetailDocument::PullRequest(detail));
            }
        }
        AppCommand::OpenIssue(number) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let detail = request(
                app,
                terminal,
                format!("Loading issue #{number}"),
                command,
                || provider.issue(&id, number),
            )?;
            if let Some(detail) = detail {
                app.open_detail(DetailDocument::Issue(detail));
            }
        }
        AppCommand::OpenWorkflowRun(run_id) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let detail = request(
                app,
                terminal,
                format!("Loading workflow run {run_id}"),
                command,
                || provider.workflow_run(&id, run_id),
            )?;
            if let Some(detail) = detail {
                app.open_detail(DetailDocument::WorkflowRun(detail));
            }
        }
        AppCommand::OpenRelease(release_id) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let detail = request(
                app,
                terminal,
                format!("Loading release {release_id}"),
                command,
                || provider.release(&id, release_id),
            )?;
            if let Some(detail) = detail {
                app.open_detail(DetailDocument::Release(detail));
            }
        }
        AppCommand::LoadBranches => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let branches = request(
                app,
                terminal,
                "Loading branches".to_owned(),
                command,
                || provider.branches(&id),
            )?;
            if let Some(branches) = branches {
                app.set_branches(branches);
            }
        }
        AppCommand::SwitchBranch(branch) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let root = String::new();
            let entries = request_with_context(
                app,
                terminal,
                format!("Switching to {branch}"),
                command,
                RequestContext::Branch(branch.clone()),
                || provider.contents(&id, &root, &branch),
            )?;
            if let Some(entries) = entries {
                app.switch_branch(branch.clone(), root, entries);
                app.set_status(format!("Switched to branch {branch}"));
            }
        }
        AppCommand::LoadTreeForSearch => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.selected_ref.clone();
            let tree = request(
                app,
                terminal,
                "Loading repository file index".to_owned(),
                command,
                || provider.tree(&id, &git_ref),
            )?;
            if let Some(tree) = tree {
                app.set_tree_and_open_search(tree);
            }
        }
        AppCommand::SearchCode { query, mode } => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.selected_ref.clone();
            let results = cancellable_code_search(
                app,
                terminal,
                provider.clone(),
                id,
                git_ref,
                query.clone(),
                mode,
                command,
            )?;
            if let Some(results) = results {
                app.set_code_search_results(query, mode, results);
            }
        }
        AppCommand::LoadBlame => {
            let (id, git_ref) = match app.repository.as_ref() {
                Some(repository) => (
                    repository.repository.id.clone(),
                    repository.selected_ref.clone(),
                ),
                None => return Ok(()),
            };
            let path = match app.file.as_ref() {
                Some(file) => file.path.clone(),
                None => return Ok(()),
            };
            let ranges = request(
                app,
                terminal,
                format!("Loading blame for {path}"),
                command,
                || provider.blame(&id, &git_ref, &path),
            )?;
            if let Some(ranges) = ranges {
                app.set_blame(ranges);
            }
        }
        AppCommand::LoadFileHistory => {
            let (id, git_ref) = match app.repository.as_ref() {
                Some(repository) => (
                    repository.repository.id.clone(),
                    repository.selected_ref.clone(),
                ),
                None => return Ok(()),
            };
            let path = match app.file.as_ref() {
                Some(file) => file.path.clone(),
                None => return Ok(()),
            };
            let commits = request(
                app,
                terminal,
                format!("Loading history for {path}"),
                command,
                || provider.file_history(&id, &git_ref, &path, 1, 100),
            )?;
            if let Some(commits) = commits {
                app.set_file_history(commits);
            }
        }
        AppCommand::AuthenticateCli => authenticate_cli(app, provider, history, terminal)?,
        AppCommand::SetToken { token, persist } => {
            set_token(token, persist, app, provider, history, terminal)?;
        }
        AppCommand::CopyText(text) => match clipboard::copy_text(&text) {
            Ok(()) => app.set_status(format!(
                "Copied {} chars to clipboard",
                text.chars().count()
            )),
            Err(error) => app.show_error("Clipboard", error.to_string()),
        },
        AppCommand::PasteClipboard => match clipboard::paste_text() {
            Ok(text) => app.handle_paste(text),
            Err(error) => app.show_error("Clipboard", error.to_string()),
        },
        AppCommand::ExportFile => {
            let Some(repository) = app.current_repository() else {
                return Ok(());
            };
            let Some(git_ref) = app.current_ref() else {
                return Ok(());
            };
            let Some(file) = app.file.as_ref() else {
                return Ok(());
            };
            match export::export_file(repository, git_ref, &file.path, &file.content) {
                Ok(path) => app.set_status(format!("Exported {}", path.display())),
                Err(error) => app.show_error("Print export", error.to_string()),
            }
        }
        AppCommand::ExportCommit => {
            let Some(repository) = app.current_repository() else {
                return Ok(());
            };
            let Some(commit) = app.commit.as_ref() else {
                return Ok(());
            };
            match export::export_commit(repository, &commit.detail) {
                Ok(path) => app.set_status(format!("Exported {}", path.display())),
                Err(error) => app.show_error("Print export", error.to_string()),
            }
        }
        AppCommand::PersistSettings => {
            if let Err(error) = save_settings(&app.settings) {
                app.show_error("Settings", error.to_string());
            }
        }
        AppCommand::OpenExternal(url) => match open_external(&url) {
            Ok(()) => app.set_status("Opened in browser"),
            Err(error) => app.show_error("Open browser", error.to_string()),
        },
    }
    app.set_cache_status(provider.cache_status_line());
    Ok(())
}

fn load_repository_tab(
    tab: RepositoryTab,
    retry: AppCommand,
    app: &mut App,
    provider: &GitHubProvider,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    let Some(repository) = app.repository.as_ref() else {
        return Ok(());
    };
    let id = repository.repository.id.clone();
    let git_ref = repository.selected_ref.clone();
    let pull_request_filter = repository.pull_request_filter;
    let issue_filter = repository.issue_filter;
    match tab {
        RepositoryTab::Code => {}
        RepositoryTab::Commits => {
            let commits = request(app, terminal, "Loading commits".to_owned(), retry, || {
                provider.commits(&id, &git_ref, 1, 50)
            })?;
            if let Some(commits) = commits {
                app.set_commits(1, commits);
            }
        }
        RepositoryTab::PullRequests => {
            let values = request(
                app,
                terminal,
                format!("Loading {} pull requests", pull_request_filter.label()),
                retry,
                || provider.pull_requests(&id, pull_request_filter),
            )?;
            if let Some(values) = values {
                app.set_pull_requests(pull_request_filter, values);
            }
        }
        RepositoryTab::Issues => {
            let values = request(
                app,
                terminal,
                format!("Loading {} issues", issue_filter.label()),
                retry,
                || provider.issues(&id, issue_filter),
            )?;
            if let Some(values) = values {
                app.set_issues(issue_filter, values);
            }
        }
        RepositoryTab::Actions => {
            let values = request(
                app,
                terminal,
                "Loading workflow runs".to_owned(),
                retry,
                || provider.workflow_runs(&id, &git_ref),
            )?;
            if let Some(values) = values {
                app.set_workflow_runs(values);
            }
        }
        RepositoryTab::Releases => {
            let values = request(app, terminal, "Loading releases".to_owned(), retry, || {
                provider.releases(&id)
            })?;
            if let Some(values) = values {
                app.set_releases(values);
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn open_repository(
    id: RepositoryId,
    resume_path: Option<String>,
    resume_screen: HistoryScreen,
    retry: AppCommand,
    app: &mut App,
    provider: &GitHubProvider,
    history: &mut HistoryStore,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    let Some(repository) = request_with_context(
        app,
        terminal,
        format!("Opening {id}"),
        retry.clone(),
        RequestContext::Repository(id.clone()),
        || provider.repository(&id),
    )?
    else {
        return Ok(());
    };

    let directory_path = match resume_screen {
        HistoryScreen::Code => resume_path.clone().unwrap_or_default(),
        HistoryScreen::File => resume_path.as_deref().map_or_else(String::new, parent_path),
        HistoryScreen::Commits | HistoryScreen::Commit => String::new(),
    };

    let Some(entries) = request_with_context(
        app,
        terminal,
        format!("Loading {}/{}", id, display_path(&directory_path)),
        retry.clone(),
        RequestContext::Directory(directory_path.clone()),
        || provider.contents(&id, &directory_path, &repository.default_branch),
    )?
    else {
        return Ok(());
    };

    app.open_repository(repository.clone(), directory_path, entries);
    if let Err(error) = history.record_repository(&repository, resume_path.clone(), resume_screen) {
        app.set_status(format!("History save failed: {error}"));
    }
    app.update_history(history.entries().to_vec());

    match (resume_screen, resume_path) {
        (HistoryScreen::File, Some(path)) => {
            let content = request_with_context(
                app,
                terminal,
                format!("Restoring {path}"),
                retry,
                RequestContext::File(path.clone()),
                || provider.file_content(&id, &path, &repository.default_branch),
            )?;
            if let Some(content) = content {
                app.open_file(path, content, None, None);
            }
        }
        (HistoryScreen::Commits, _) => {
            let commits = request(app, terminal, "Restoring commits".to_owned(), retry, || {
                provider.commits(&id, &repository.default_branch, 1, 50)
            })?;
            if let Some(commits) = commits {
                app.set_commits(1, commits);
            }
        }
        (HistoryScreen::Commit, Some(sha)) => {
            let detail = request(
                app,
                terminal,
                format!("Restoring commit {}", short_sha(&sha)),
                retry,
                || provider.commit(&id, &sha),
            )?;
            if let Some(detail) = detail {
                app.open_commit(detail);
            }
        }
        (HistoryScreen::Code | HistoryScreen::File | HistoryScreen::Commit, _) => {}
    }
    Ok(())
}

fn refresh_home(
    retry: AppCommand,
    app: &mut App,
    provider: &GitHubProvider,
    history: &HistoryStore,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    let pushed_after = (Utc::now() - ChronoDuration::days(30))
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let featured_query = format!("stars:>1000 pushed:>{pushed_after} archived:false");

    if let Some(featured) = request(
        app,
        terminal,
        "Refreshing Featured".to_owned(),
        retry.clone(),
        || provider.search_repositories(&featured_query, "updated", 8),
    )? {
        if !featured.is_empty() {
            app.home.featured = featured;
            app.home.featured_index = 0;
        }
    } else {
        return Ok(());
    }

    let recommended_query = history.top_language().map_or_else(
        || "topic:terminal stars:>100 archived:false".to_owned(),
        |language| format!("language:\"{language}\" stars:>300 archived:false"),
    );
    if let Some(recommended) = request(
        app,
        terminal,
        "Refreshing Recommended".to_owned(),
        retry,
        || provider.search_repositories(&recommended_query, "stars", 8),
    )? {
        if !recommended.is_empty() {
            app.home.recommended = recommended;
            app.home.recommended_index = 0;
        }
        app.set_status("Recommendations refreshed");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cancellable_code_search(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    provider: GitHubProvider,
    id: RepositoryId,
    git_ref: String,
    query: String,
    mode: CodeSearchMode,
    retry: AppCommand,
) -> Result<Option<Vec<CodeSearchResult>>> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel_on_drop = CancelSearchOnDrop(Arc::clone(&cancelled));
    let worker_cancelled = Arc::clone(&cancelled);
    let (sender, receiver) = mpsc::channel();
    let initial_message = format!(
        "{} search: querying GitHub for {query} · Esc/Ctrl+C cancels",
        code_search_mode_label(mode)
    );
    app.loading = Some(initial_message);
    terminal.draw(|frame| ui::draw(frame, app))?;

    let _search_worker = thread::spawn(move || {
        let result = enriched_code_search(
            &provider,
            &id,
            &git_ref,
            &query,
            mode,
            worker_cancelled.as_ref(),
            |message| {
                let _ = sender.send(CodeSearchWorkerMessage::Progress(message));
            },
        );
        let _ = sender.send(CodeSearchWorkerMessage::Finished(result));
    });

    loop {
        let mut redraw = false;
        loop {
            match receiver.try_recv() {
                Ok(CodeSearchWorkerMessage::Progress(message)) => {
                    app.loading = Some(format!("{message} · Esc/Ctrl+C cancels"));
                    redraw = true;
                }
                Ok(CodeSearchWorkerMessage::Finished(result)) => {
                    app.loading = None;
                    return match result {
                        Ok(ApiResponse { value, rate_limit }) => {
                            app.update_rate_limit(rate_limit);
                            Ok(Some(value))
                        }
                        Err(error) => {
                            if let Some(rate_limit) = error.rate_limit() {
                                app.update_rate_limit(rate_limit.clone());
                            }
                            handle_provider_error(app, error, retry, RequestContext::Auto);
                            Ok(None)
                        }
                    };
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    app.loading = None;
                    app.show_error(
                        "Code search",
                        "The background search stopped unexpectedly before returning a result",
                    );
                    return Ok(None);
                }
            }
        }
        if redraw {
            terminal.draw(|frame| ui::draw(frame, app))?;
        }

        if !event::poll(Duration::from_millis(60))? {
            continue;
        }

        match event::read()? {
            Event::Key(key)
                if !key.is_release()
                    && (key.code == KeyCode::Esc
                        || (key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('c'))) =>
            {
                mark_code_search_cancelled(app, cancelled.as_ref(), mode);
                return Ok(None);
            }
            Event::Mouse(mouse)
                if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right)) =>
            {
                mark_code_search_cancelled(app, cancelled.as_ref(), mode);
                return Ok(None);
            }
            Event::Resize(_, _) => {
                terminal.draw(|frame| ui::draw(frame, app))?;
            }
            _ => {}
        }
    }
}

fn enriched_code_search(
    provider: &GitHubProvider,
    id: &RepositoryId,
    git_ref: &str,
    query: &str,
    mode: CodeSearchMode,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(String),
) -> ProviderResult<Vec<CodeSearchResult>> {
    let mut rate_limit = RateLimit::default();
    let mut deferred_error = None;
    progress(format!(
        "{} search: querying GitHub code index",
        code_search_mode_label(mode)
    ));
    let api_candidates = match provider.search_code(id, query, 50) {
        Ok(ApiResponse {
            value,
            rate_limit: search_rate_limit,
        }) => {
            update_rate_limit(&mut rate_limit, search_rate_limit);
            value
        }
        Err(ProviderError::AuthenticationRequired {
            rate_limit: limited,
        }) => {
            update_rate_limit(&mut rate_limit, limited);
            Vec::new()
        }
        Err(ProviderError::Api {
            status: 401 | 403,
            rate_limit: limited,
            ..
        }) => {
            update_rate_limit(&mut rate_limit, limited);
            Vec::new()
        }
        Err(error) => {
            deferred_error = Some(error);
            Vec::new()
        }
    };

    let mut results = Vec::new();
    if cancelled.load(Ordering::Relaxed) {
        return Ok(ApiResponse {
            value: results,
            rate_limit,
        });
    }

    let indexed_limit = match mode {
        CodeSearchMode::Definition => 24,
        CodeSearchMode::Text => 50,
    };
    let indexed_total = api_candidates.len().min(indexed_limit);
    let mut scanned = HashSet::new();
    for (index, candidate) in api_candidates.iter().take(indexed_limit).enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(ApiResponse {
                value: results,
                rate_limit,
            });
        }
        progress(format!(
            "{} search: indexed file {}/{} · {}",
            code_search_mode_label(mode),
            index + 1,
            indexed_total,
            candidate.path
        ));
        scanned.insert(candidate.path.clone());
        if let Ok(ApiResponse {
            value: content,
            rate_limit: file_rate_limit,
        }) = provider.file_content(id, &candidate.path, git_ref)
        {
            update_rate_limit(&mut rate_limit, file_rate_limit);
            append_code_search_matches(&mut results, mode, query, candidate, &content);
        }
        if code_search_limit_reached(mode, results.len()) {
            break;
        }
    }

    if cancelled.load(Ordering::Relaxed) {
        return Ok(ApiResponse {
            value: results,
            rate_limit,
        });
    }

    let needs_local_fallback = results.is_empty()
        || (mode == CodeSearchMode::Definition && results.len() < 8)
        || (mode == CodeSearchMode::Text && results.len() < 20);
    if needs_local_fallback {
        progress(format!(
            "{} search: loading repository file index",
            code_search_mode_label(mode)
        ));
        match provider.tree(id, git_ref) {
            Ok(ApiResponse {
                value: tree,
                rate_limit: tree_rate_limit,
            }) => {
                update_rate_limit(&mut rate_limit, tree_rate_limit);
                let candidates = local_search_candidates(
                    tree,
                    query,
                    &scanned,
                    provider.is_authenticated(),
                    mode,
                );
                let candidate_count = candidates.len();
                for (index, entry) in candidates.into_iter().enumerate() {
                    if cancelled.load(Ordering::Relaxed) {
                        return Ok(ApiResponse {
                            value: results,
                            rate_limit,
                        });
                    }
                    progress(format!(
                        "{} search: repository file {}/{} · {}",
                        code_search_mode_label(mode),
                        index + 1,
                        candidate_count,
                        entry.path
                    ));
                    let candidate = tree_entry_as_search_result(entry);
                    if let Ok(ApiResponse {
                        value: content,
                        rate_limit: file_rate_limit,
                    }) = provider.file_content(id, &candidate.path, git_ref)
                    {
                        update_rate_limit(&mut rate_limit, file_rate_limit);
                        append_code_search_matches(&mut results, mode, query, &candidate, &content);
                    }
                    if code_search_limit_reached(mode, results.len()) {
                        break;
                    }
                }
            }
            Err(tree_error) if api_candidates.is_empty() && results.is_empty() => {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(ApiResponse {
                        value: results,
                        rate_limit,
                    });
                }
                return Err(deferred_error.unwrap_or(tree_error));
            }
            Err(_) => {}
        }
    }

    let mut seen = HashSet::new();
    results.retain(|result| seen.insert((result.path.clone(), result.line)));
    results.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
    });

    Ok(ApiResponse {
        value: results,
        rate_limit,
    })
}

fn code_search_mode_label(mode: CodeSearchMode) -> &'static str {
    match mode {
        CodeSearchMode::Definition => "Definition",
        CodeSearchMode::Text => "Code",
    }
}

fn mark_code_search_cancelled(app: &mut App, cancelled: &AtomicBool, mode: CodeSearchMode) {
    cancelled.store(true, Ordering::Relaxed);
    app.loading = None;
    app.modal = None;
    app.set_status(format!(
        "{} search cancelled; an in-flight request may finish in the background, but its result will be ignored",
        code_search_mode_label(mode)
    ));
}

fn append_code_search_matches(
    results: &mut Vec<CodeSearchResult>,
    mode: CodeSearchMode,
    query: &str,
    candidate: &CodeSearchResult,
    content: &str,
) {
    match mode {
        CodeSearchMode::Definition => {
            if let Some(symbol) = symbols::find_definition(&candidate.path, content, query) {
                results.push(CodeSearchResult {
                    name: symbol.name,
                    path: candidate.path.clone(),
                    sha: candidate.sha.clone(),
                    html_url: candidate.html_url.clone(),
                    line: Some(symbol.line),
                    preview: content
                        .lines()
                        .nth(symbol.line.saturating_sub(1))
                        .map(code_preview),
                    kind: Some(symbol.kind),
                });
            }
        }
        CodeSearchMode::Text => {
            for (line, preview) in symbols::text_matches(content, query, 4) {
                results.push(CodeSearchResult {
                    name: candidate.name.clone(),
                    path: candidate.path.clone(),
                    sha: candidate.sha.clone(),
                    html_url: candidate.html_url.clone(),
                    line: Some(line),
                    preview: Some(code_preview(&preview)),
                    kind: Some("match".to_owned()),
                });
            }
        }
    }
}

fn local_search_candidates(
    tree: Vec<TreeEntry>,
    query: &str,
    excluded: &HashSet<String>,
    authenticated: bool,
    mode: CodeSearchMode,
) -> Vec<TreeEntry> {
    let query = query.to_ascii_lowercase();
    let mut candidates = tree
        .into_iter()
        .filter(TreeEntry::is_file)
        .filter(|entry| entry.size.unwrap_or(0) <= 1_500_000)
        .filter(|entry| symbols::is_searchable_path(&entry.path))
        .filter(|entry| !excluded.contains(&entry.path))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|entry| {
        let path = entry.path.to_ascii_lowercase();
        (
            !path.contains(&query),
            path.matches('/').count(),
            entry.size.unwrap_or(u64::MAX),
            path,
        )
    });
    candidates.truncate(match (mode, authenticated) {
        (CodeSearchMode::Definition, true) => 60,
        (CodeSearchMode::Definition, false) => 24,
        (CodeSearchMode::Text, true) => 120,
        (CodeSearchMode::Text, false) => 28,
    });
    candidates
}

fn tree_entry_as_search_result(entry: TreeEntry) -> CodeSearchResult {
    let name = entry
        .path
        .rsplit('/')
        .next()
        .unwrap_or(&entry.path)
        .to_owned();
    CodeSearchResult {
        name,
        path: entry.path,
        sha: entry.sha,
        html_url: String::new(),
        line: None,
        preview: None,
        kind: None,
    }
}

fn code_search_limit_reached(mode: CodeSearchMode, len: usize) -> bool {
    match mode {
        CodeSearchMode::Definition => len >= 16,
        CodeSearchMode::Text => len >= 120,
    }
}

fn update_rate_limit(current: &mut RateLimit, newer: RateLimit) {
    if rate_limit_has_values(&newer) {
        *current = newer;
    }
}

fn code_preview(line: &str) -> String {
    const MAX_CHARS: usize = 180;
    let line = line.trim();
    let mut preview = line.chars().take(MAX_CHARS).collect::<String>();
    if line.chars().count() > MAX_CHARS {
        preview.push_str(" [truncated]");
    }
    preview
}

fn rate_limit_has_values(rate_limit: &RateLimit) -> bool {
    rate_limit.limit.is_some()
        || rate_limit.remaining.is_some()
        || rate_limit.reset_epoch.is_some()
        || rate_limit.resource.is_some()
}

fn authenticate_cli(
    app: &mut App,
    provider: &mut GitHubProvider,
    history: &mut HistoryStore,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    disable_mouse_capture();
    ratatui::try_restore().context("Could not restore terminal before authentication")?;
    println!("RepoTrek GitHub authentication");
    println!("Authorize RepoTrek through GitHub CLI in the browser.\n");
    let authentication = auth::authenticate_with_github_cli();
    *terminal =
        ratatui::try_init().context("Could not reinitialize terminal after authentication")?;
    let _ = enable_mouse_capture();

    match authentication {
        Ok(token) => finish_token_auth(token, false, app, provider, history, terminal)?,
        Err(error) => app.show_error("GitHub authentication", error.to_string()),
    }
    Ok(())
}

fn set_token(
    token: String,
    persist: bool,
    app: &mut App,
    provider: &mut GitHubProvider,
    history: &mut HistoryStore,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    finish_token_auth(token, persist, app, provider, history, terminal)
}

fn finish_token_auth(
    token: String,
    persist: bool,
    app: &mut App,
    provider: &mut GitHubProvider,
    history: &mut HistoryStore,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    provider.set_token(token.clone());
    match provider.viewer_login() {
        Ok(ApiResponse {
            value: login,
            rate_limit,
        }) => {
            app.update_rate_limit(rate_limit);
            app.authenticated = true;
            app.auth_user = Some(login.clone());
            app.modal = None;

            let persistence = if persist {
                Some(auth::save_token_persistently(&token))
            } else {
                None
            };

            if let Some(retry) = app.pending_retry.take() {
                execute_command(retry, app, provider, history, terminal)?;
            }

            match persistence {
                Some(Ok(store)) => app.set_status(format!(
                    "GitHub authentication enabled for @{login}; saved in {store}"
                )),
                Some(Err(error)) => app.set_status(format!(
                    "Authenticated as @{login}; credential save failed: {error}"
                )),
                None => app.set_status(format!("GitHub authentication enabled for @{login}")),
            }
        }
        Err(error) => {
            provider.clear_token();
            app.authenticated = false;
            app.auth_user = None;
            app.show_error("GitHub token", format!("Token validation failed: {error}"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
enum RequestContext {
    Auto,
    Repository(RepositoryId),
    Directory(String),
    File(String),
    Branch(String),
}

fn request<T>(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    message: String,
    retry: AppCommand,
    operation: impl FnOnce() -> ProviderResult<T>,
) -> Result<Option<T>> {
    request_with_context(
        app,
        terminal,
        message,
        retry,
        RequestContext::Auto,
        operation,
    )
}

fn request_with_context<T>(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    message: String,
    retry: AppCommand,
    context: RequestContext,
    operation: impl FnOnce() -> ProviderResult<T>,
) -> Result<Option<T>> {
    app.loading = Some(message);
    terminal.draw(|frame| ui::draw(frame, app))?;
    let result = operation();
    app.loading = None;

    match result {
        Ok(ApiResponse { value, rate_limit }) => {
            app.update_rate_limit(rate_limit);
            Ok(Some(value))
        }
        Err(error) => {
            if let Some(rate_limit) = error.rate_limit() {
                app.update_rate_limit(rate_limit.clone());
            }
            handle_provider_error(app, error, retry, context);
            Ok(None)
        }
    }
}

fn handle_provider_error(
    app: &mut App,
    error: ProviderError,
    retry: AppCommand,
    context: RequestContext,
) {
    if matches!(&error, ProviderError::Api { status: 404, .. }) {
        show_not_found(app, &retry, context);
        return;
    }

    match error {
        ProviderError::AuthenticationRequired { .. } if !app.authenticated => {
            app.show_auth_required(retry);
        }
        ProviderError::AuthenticationRequired { .. } => {
            app.show_error(
                "GitHub authentication",
                "The current token does not allow this operation",
            );
        }
        ProviderError::RateLimited(rate_limit) if !app.authenticated => {
            app.show_rate_limit(rate_limit, retry);
        }
        ProviderError::RateLimited(rate_limit) => {
            let reset = rate_limit.reset_at().map_or_else(
                || "unknown".to_owned(),
                |time| time.with_timezone(&chrono::Local).to_rfc3339(),
            );
            app.show_error(
                "GitHub API rate limit",
                format!("Authenticated API quota exhausted. Reset: {reset}"),
            );
        }
        ProviderError::TemporarilyLimited {
            message,
            retry_after_seconds,
            ..
        } => {
            let retry_after = retry_after_seconds
                .map(|seconds| format!(" Retry-After: {seconds}s"))
                .unwrap_or_default();
            app.show_error(
                "GitHub API temporary limit",
                format!("{message}{retry_after}"),
            );
        }
        other => app.show_error("GitHub", other.to_string()),
    }
}

fn show_not_found(app: &mut App, retry: &AppCommand, context: RequestContext) {
    let private_hint = if app.authenticated {
        "GitHub also returns 404 when the current token cannot access a private repository. Check the token's repository permissions."
    } else {
        "GitHub also returns 404 for private repositories. Press F2 to authenticate if the repository is not public."
    };

    match context {
        RequestContext::Repository(id) => app.show_error(
            "Repository not found",
            format!(
                "Could not find `{}`. Check the owner/repository spelling.\n\n{private_hint}",
                id.full_name()
            ),
        ),
        RequestContext::Directory(path) => app.show_error(
            "Directory not found",
            format!(
                "`{}` does not exist at the selected branch or commit. Refresh the repository if it changed recently.",
                if path.is_empty() { "/" } else { path.as_str() }
            ),
        ),
        RequestContext::File(path) => app.show_error(
            "File not found",
            format!(
                "`{path}` does not exist at the selected branch or commit. It may have been renamed or removed."
            ),
        ),
        RequestContext::Branch(branch) => app.show_error(
            "Branch not found",
            format!("The branch or tag `{branch}` no longer exists."),
        ),
        RequestContext::Auto => match retry {
            AppCommand::OpenRepository { id, .. } => app.show_error(
                "Repository not found",
                format!(
                    "Could not find `{}`. Check the owner/repository spelling.\n\n{private_hint}",
                    id.full_name()
                ),
            ),
            AppCommand::OpenFile { path, .. } => app.show_error(
                "File not found",
                format!(
                    "`{path}` does not exist at the selected branch or commit. It may have been renamed or removed."
                ),
            ),
            AppCommand::OpenDirectory(path) => app.show_error(
                "Directory not found",
                format!(
                    "`{}` does not exist at the selected branch or commit. Refresh the repository if it changed recently.",
                    if path.is_empty() { "/" } else { path.as_str() }
                ),
            ),
            AppCommand::SwitchBranch(branch) => app.show_error(
                "Branch not found",
                format!("The branch or tag `{branch}` no longer exists."),
            ),
            _ => app.show_error(
                "GitHub item not found",
                format!(
                    "The requested repository item does not exist at the selected revision.\n\n{private_hint}"
                ),
            ),
        },
    }
}

fn update_history_location(
    app: &mut App,
    history: &mut HistoryStore,
    id: &RepositoryId,
    path: Option<String>,
    screen: HistoryScreen,
) {
    if let Err(error) = history.update_location(id, path, screen) {
        app.set_status(format!("History save failed: {error}"));
    }
    app.update_history(history.entries().to_vec());
}

fn open_external(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status()?;
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = Command::new("xdg-open").arg(url).status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("Browser command returned a non-zero status")
    }
}

fn parent_path(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(parent, _)| parent.to_owned())
}

fn smart_repository_query(query: &str) -> String {
    let terms = query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        query.to_owned()
    } else {
        format!("{} in:name,description,readme", terms.join(" "))
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() { "/" } else { path }
}

fn short_sha(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        disable_mouse_capture();
        let _ = ratatui::try_restore();
        original(panic_info);
    }));
}
