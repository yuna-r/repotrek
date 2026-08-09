mod app;
mod auth;
mod cli;
mod clipboard;
mod diff;
mod export;
mod highlight;
mod icons;
mod intelligence;
mod model;
mod provider;
mod settings;
mod storage;
mod symbols;
mod theme;
mod ui;

use std::{io::stdout, process::Command, str::FromStr, time::Duration};

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use clap::Parser;
use crossterm::{
    event::{
        self, Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::supports_keyboard_enhancement,
};
use ratatui::DefaultTerminal;

use crate::{
    app::{App, AppCommand, DetailDocument, RepositoryTab},
    cli::Cli,
    icons::Icons,
    model::{ApiResponse, HistoryScreen, RepositoryId},
    provider::{ProviderError, ProviderResult, RepositoryProvider, github::GitHubProvider},
    settings::{SettingsStore, save_settings},
    storage::HistoryStore,
};

fn main() -> Result<()> {
    install_panic_hook();
    let cli = Cli::parse();

    if let Some(cmd) = cli.command {
        let engine = intelligence::IntelligenceEngine::new();
        let current_dir = std::env::current_dir()?;
        let (index, findings, health) = engine.analyze_local(&current_dir)?;

        let format = if cli.json {
            intelligence::ReportFormat::Json
        } else if cli.sarif {
            intelligence::ReportFormat::Sarif
        } else if cli.markdown {
            intelligence::ReportFormat::Markdown
        } else if cli.html {
            intelligence::ReportFormat::Html
        } else {
            intelligence::ReportFormat::Json
        };

        match cmd {
            cli::Commands::Intelligence { .. } | cli::Commands::Health { .. } => {
                if cli.json || cli.sarif || cli.markdown || cli.html {
                    println!("{}", intelligence::format_report(format, &index, &findings));
                } else {
                    println!("====================================================");
                    println!("REPO TREK INTELLIGENCE ANALYSIS");
                    println!("====================================================");
                    println!("Repository: {}", index.repo_name);
                    println!("Overall Health Score: {}/100 ({})", health.overall, health.status);
                    println!("\nCATEGORY BREAKDOWN:");
                    for c in &health.categories {
                        println!("  - {:<15}: {}/100 ({})", c.name, c.score, c.status);
                    }
                    println!("\nFINDINGS SUMMARY:");
                    println!("  Total Findings: {}", findings.len());
                    println!("  Critical: {}", health.critical_count);
                    println!("  High: {}", health.high_count);
                    println!("====================================================");
                }
            }
            cli::Commands::Architecture { .. } => {
                let arch_findings: Vec<_> = findings.iter().filter(|f| f.analyzer == "ArchitectureAnalyzer").collect();
                println!("{}", serde_json::to_string_pretty(&arch_findings)?);
            }
            cli::Commands::Dependencies { .. } => {
                let dep_findings: Vec<_> = findings.iter().filter(|f| f.analyzer == "DependencyAnalyzer").collect();
                println!("{}", serde_json::to_string_pretty(&dep_findings)?);
            }
            cli::Commands::Security { .. } => {
                let sec_findings: Vec<_> = findings.iter().filter(|f| f.analyzer == "SecurityAnalyzer" || f.analyzer == "VulnerabilityAnalyzer").collect();
                println!("{}", serde_json::to_string_pretty(&sec_findings)?);
            }
            cli::Commands::Quality { .. } => {
                let qual_findings: Vec<_> = findings.iter().filter(|f| f.analyzer == "QualityAnalyzer").collect();
                println!("{}", serde_json::to_string_pretty(&qual_findings)?);
            }
            cli::Commands::Onboard { .. } => {
                let guide = intelligence::analyzers::onboarding::OnboardingAnalyzer::generate_guide(&index);
                println!("{}", serde_json::to_string_pretty(&guide)?);
            }
            cli::Commands::Report { .. } => {
                if cli.json || cli.sarif || cli.markdown || cli.html {
                    println!("{}", intelligence::format_report(format, &index, &findings));
                } else {
                    println!("{}", intelligence::RepositoryReport::generate(&index, &findings));
                }
            }
            cli::Commands::Ai { query } => {
                let gateway = intelligence::AiGateway::new(intelligence::PrivacyMode::Default);
                let response = gateway.ask(&index, &findings, &query)?;
                println!("{}", response);
            }
            cli::Commands::Mcp => {
                let server = intelligence::McpServer::new(index, findings);
                server.run_stdio()?;
            }
        }
        return Ok(());
    }

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
    let keyboard_enhancement = enable_keyboard_enhancement();
    let result = run(
        &mut terminal,
        &mut app,
        &mut provider,
        &mut history,
        cli.repository,
        cli.no_home_refresh,
    );
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
            let entries = request(
                app,
                terminal,
                format!("Loading {}/{}", id, display_path(&path)),
                command,
                || provider.contents(&id, &path, &git_ref),
            )?;
            if let Some(entries) = entries {
                app.set_directory(path.clone(), entries);
                update_history_location(app, history, &id, Some(path), HistoryScreen::Code);
            }
        }
        AppCommand::OpenFile { path, find } => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.selected_ref.clone();
            let content = request(app, terminal, format!("Loading {path}"), command, || {
                provider.file_content(&id, &path, &git_ref)
            })?;
            if let Some(content) = content {
                app.open_file(path.clone(), content, find.as_deref());
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
            let entries = request(
                app,
                terminal,
                format!("Switching to {branch}"),
                command,
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
            let results = request(
                app,
                terminal,
                format!("Searching code: {query}"),
                command,
                || provider.search_code(&id, &query, 100),
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
                "Loading pull requests".to_owned(),
                retry,
                || provider.pull_requests(&id),
            )?;
            if let Some(values) = values {
                app.set_pull_requests(values);
            }
        }
        RepositoryTab::Issues => {
            let values = request(app, terminal, "Loading issues".to_owned(), retry, || {
                provider.issues(&id)
            })?;
            if let Some(values) = values {
                app.set_issues(values);
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
        RepositoryTab::Intelligence => {}
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
    let Some(repository) = request(
        app,
        terminal,
        format!("Opening {id}"),
        retry.clone(),
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

    let Some(entries) = request(
        app,
        terminal,
        format!("Loading {}/{}", id, display_path(&directory_path)),
        retry.clone(),
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
            let content = request(app, terminal, format!("Restoring {path}"), retry, || {
                provider.file_content(&id, &path, &repository.default_branch)
            })?;
            if let Some(content) = content {
                app.open_file(path, content, None);
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

fn authenticate_cli(
    app: &mut App,
    provider: &mut GitHubProvider,
    history: &mut HistoryStore,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    ratatui::try_restore().context("Could not restore terminal before authentication")?;
    println!("RepoTrek GitHub authentication");
    println!("Authorize RepoTrek through GitHub CLI in the browser.\n");
    let authentication = auth::authenticate_with_github_cli();
    *terminal =
        ratatui::try_init().context("Could not reinitialize terminal after authentication")?;

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

fn request<T>(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    message: String,
    retry: AppCommand,
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
            handle_provider_error(app, error, retry);
            Ok(None)
        }
    }
}

fn handle_provider_error(app: &mut App, error: ProviderError, retry: AppCommand) {
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
        let _ = ratatui::try_restore();
        original(panic_info);
    }));
}
