mod app;
mod auth;
mod cli;
mod export;
mod highlight;
mod icons;
mod model;
mod provider;
mod storage;
mod ui;

use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use clap::Parser;
use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use crate::{
    app::{App, AppCommand},
    cli::Cli,
    icons::Icons,
    model::{ApiResponse, HistoryScreen, RepositoryId},
    provider::{ProviderError, ProviderResult, RepositoryProvider, github::GitHubProvider},
    storage::HistoryStore,
};

fn main() -> Result<()> {
    install_panic_hook();
    let cli = Cli::parse();
    let token = auth::token_from_environment(cli.anonymous);
    let mut provider = GitHubProvider::new(token)?;
    let mut history = HistoryStore::load()?;
    let icons = Icons::new(cli.emoji);
    let mut app = App::new(
        history.entries().to_vec(),
        icons,
        provider.is_authenticated(),
    );

    let mut terminal = ratatui::try_init().context("ターミナルを初期化できません")?;
    let result = run(
        &mut terminal,
        &mut app,
        &mut provider,
        &mut history,
        cli.repository,
        cli.no_home_refresh,
    );
    let restore_result = ratatui::try_restore().context("ターミナルを復元できません");

    match (result, restore_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
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
        } => {
            open_repository(
                id,
                resume_path,
                resume_screen,
                command,
                app,
                provider,
                history,
                terminal,
            )?;
        }
        AppCommand::RefreshHome => {
            refresh_home(command, app, provider, history, terminal)?;
        }
        AppCommand::OpenDirectory(path) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.repository.default_branch.clone();
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
        AppCommand::OpenFile(path) => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.repository.default_branch.clone();
            let content = request(app, terminal, format!("Loading {path}"), command, || {
                provider.file_content(&id, &path, &git_ref)
            })?;
            if let Some(content) = content {
                app.open_file(path.clone(), content);
                update_history_location(app, history, &id, Some(path), HistoryScreen::File);
            }
        }
        AppCommand::LoadCommits { page } => {
            let Some(repository) = app.repository.as_ref() else {
                return Ok(());
            };
            let id = repository.repository.id.clone();
            let git_ref = repository.repository.default_branch.clone();
            let commits = request(
                app,
                terminal,
                format!("Loading commits page {page}"),
                command,
                || provider.commits(&id, &git_ref, page, 50),
            )?;
            if let Some(commits) = commits {
                if commits.is_empty() && page > 1 {
                    app.status = Some("No more commits".to_owned());
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
        AppCommand::Authenticate => {
            authenticate(app, provider, history, terminal)?;
        }
        AppCommand::ExportFile => {
            let Some(repository) = app.current_repository() else {
                return Ok(());
            };
            let Some(file) = app.file.as_ref() else {
                return Ok(());
            };
            match export::export_file(
                repository,
                &repository.default_branch,
                &file.path,
                &file.content,
            ) {
                Ok(path) => {
                    app.status = Some(format!("Exported {}", path.display()));
                }
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
                Ok(path) => {
                    app.status = Some(format!("Exported {}", path.display()));
                }
                Err(error) => app.show_error("Print export", error.to_string()),
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
        app.status = Some(format!("History save failed: {error}"));
    }
    app.update_history(history.entries().to_vec());

    match (resume_screen, resume_path) {
        (HistoryScreen::File, Some(path)) => {
            let content = request(app, terminal, format!("Restoring {path}"), retry, || {
                provider.file_content(&id, &path, &repository.default_branch)
            })?;
            if let Some(content) = content {
                app.open_file(path, content);
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
        app.status = Some("Recommendations refreshed".to_owned());
    }
    Ok(())
}

fn authenticate(
    app: &mut App,
    provider: &mut GitHubProvider,
    history: &mut HistoryStore,
    terminal: &mut DefaultTerminal,
) -> Result<()> {
    if app.authenticated {
        app.show_error(
            "GitHub API rate limit",
            "すでに認証済みです。リセット時刻までキャッシュ済みデータを利用してください",
        );
        return Ok(());
    }

    ratatui::try_restore().context("認証前にターミナルを復元できません")?;
    println!("RepoTrek GitHub authentication");
    println!("GitHub CLIのDevice Flowを開始します。使用するGitHubアカウントで承認してください。\n");
    let authentication = auth::authenticate_with_github_cli();
    *terminal = ratatui::try_init().context("認証後にターミナルを再初期化できません")?;

    match authentication {
        Ok(token) => {
            provider.set_token(token);
            app.authenticated = true;
            app.status = Some("GitHub authentication enabled".to_owned());
            app.modal = None;
            if let Some(retry) = app.pending_retry.take() {
                execute_command(retry, app, provider, history, terminal)?;
            }
        }
        Err(error) => {
            app.show_error("GitHub authentication", error.to_string());
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
                format!("認証済みAPI上限に達しました。Reset: {reset}"),
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
        app.status = Some(format!("History save failed: {error}"));
    }
    app.update_history(history.entries().to_vec());
}

fn parent_path(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(parent, _)| parent.to_owned())
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
