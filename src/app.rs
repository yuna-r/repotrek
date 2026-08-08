use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    icons::Icons,
    model::{
        CommitDetail, CommitSummary, ContentEntry, HistoryEntry, HistoryScreen, RateLimit,
        RepoCard, Repository, RepositoryId,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Repository,
    File,
    Commit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeFocus {
    Search,
    History,
    Featured,
    Recommended,
}

impl HomeFocus {
    fn next(self) -> Self {
        match self {
            Self::Search => Self::History,
            Self::History => Self::Featured,
            Self::Featured => Self::Recommended,
            Self::Recommended => Self::Search,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Search => Self::Recommended,
            Self::History => Self::Search,
            Self::Featured => Self::History,
            Self::Recommended => Self::Featured,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryTab {
    Code,
    Commits,
}

#[derive(Debug, Clone)]
pub struct HomeState {
    pub query: String,
    pub focus: HomeFocus,
    pub history: Vec<HistoryEntry>,
    pub featured: Vec<RepoCard>,
    pub recommended: Vec<RepoCard>,
    pub history_index: usize,
    pub featured_index: usize,
    pub recommended_index: usize,
}

#[derive(Debug, Clone)]
pub struct RepositoryState {
    pub repository: Repository,
    pub path: String,
    pub entries: Vec<ContentEntry>,
    pub entry_index: usize,
    pub tab: RepositoryTab,
    pub commits: Vec<CommitSummary>,
    pub commit_index: usize,
    pub commit_page: u32,
}

impl RepositoryState {
    #[must_use]
    pub fn selected_entry(&self) -> Option<&ContentEntry> {
        self.entries.get(self.entry_index)
    }

    #[must_use]
    pub fn selected_commit(&self) -> Option<&CommitSummary> {
        self.commits.get(self.commit_index)
    }

    #[must_use]
    pub fn parent_path(&self) -> String {
        self.path
            .rsplit_once('/')
            .map_or_else(String::new, |(parent, _)| parent.to_owned())
    }
}

#[derive(Debug, Clone)]
pub struct FileState {
    pub path: String,
    pub content: String,
    pub vertical_scroll: usize,
    pub horizontal_scroll: usize,
}

impl FileState {
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.content.lines().count().max(1)
    }
}

#[derive(Debug, Clone)]
pub struct CommitState {
    pub detail: CommitDetail,
    pub vertical_scroll: usize,
}

#[derive(Debug, Clone)]
pub enum Modal {
    Help,
    Error { title: String, message: String },
    RateLimit { rate_limit: RateLimit },
}

#[derive(Debug, Clone)]
pub enum AppCommand {
    None,
    Quit,
    OpenRepository {
        id: RepositoryId,
        resume_path: Option<String>,
        resume_screen: HistoryScreen,
    },
    RefreshHome,
    OpenDirectory(String),
    OpenFile(String),
    LoadCommits {
        page: u32,
    },
    OpenCommit(String),
    Authenticate,
    ExportFile,
    ExportCommit,
}

#[derive(Debug)]
pub struct App {
    pub screen: Screen,
    pub home: HomeState,
    pub repository: Option<RepositoryState>,
    pub file: Option<FileState>,
    pub commit: Option<CommitState>,
    pub modal: Option<Modal>,
    pub loading: Option<String>,
    pub status: Option<String>,
    pub rate_limit: Option<RateLimit>,
    pub icons: Icons,
    pub authenticated: bool,
    pub pending_retry: Option<AppCommand>,
}

impl App {
    #[must_use]
    pub fn new(history: Vec<HistoryEntry>, icons: Icons, authenticated: bool) -> Self {
        Self {
            screen: Screen::Home,
            home: HomeState {
                query: String::new(),
                focus: HomeFocus::Search,
                history,
                featured: fallback_featured(),
                recommended: fallback_recommended(),
                history_index: 0,
                featured_index: 0,
                recommended_index: 0,
            },
            repository: None,
            file: None,
            commit: None,
            modal: None,
            loading: None,
            status: None,
            rate_limit: None,
            icons,
            authenticated,
            pending_retry: None,
        }
    }

    #[must_use]
    pub fn current_repository(&self) -> Option<&Repository> {
        self.repository.as_ref().map(|state| &state.repository)
    }

    pub fn update_history(&mut self, history: Vec<HistoryEntry>) {
        self.home.history = history;
        self.home.history_index = self
            .home
            .history_index
            .min(self.home.history.len().saturating_sub(1));
    }

    pub fn update_rate_limit(&mut self, rate_limit: RateLimit) {
        self.rate_limit = Some(rate_limit);
    }

    pub fn show_error(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.modal = Some(Modal::Error {
            title: title.into(),
            message: message.into(),
        });
    }

    pub fn show_rate_limit(&mut self, rate_limit: RateLimit, retry: AppCommand) {
        self.rate_limit = Some(rate_limit.clone());
        self.pending_retry = Some(retry);
        self.modal = Some(Modal::RateLimit { rate_limit });
    }

    pub fn open_repository(
        &mut self,
        repository: Repository,
        path: String,
        entries: Vec<ContentEntry>,
    ) {
        self.repository = Some(RepositoryState {
            repository,
            path,
            entries,
            entry_index: 0,
            tab: RepositoryTab::Code,
            commits: Vec::new(),
            commit_index: 0,
            commit_page: 1,
        });
        self.file = None;
        self.commit = None;
        self.screen = Screen::Repository;
    }

    pub fn set_directory(&mut self, path: String, entries: Vec<ContentEntry>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.path = path;
            repository.entries = entries;
            repository.entry_index = 0;
            repository.tab = RepositoryTab::Code;
        }
        self.screen = Screen::Repository;
    }

    pub fn open_file(&mut self, path: String, content: String) {
        self.file = Some(FileState {
            path,
            content,
            vertical_scroll: 0,
            horizontal_scroll: 0,
        });
        self.screen = Screen::File;
    }

    pub fn set_commits(&mut self, page: u32, commits: Vec<CommitSummary>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.tab = RepositoryTab::Commits;
            repository.commits = commits;
            repository.commit_index = 0;
            repository.commit_page = page;
        }
        self.screen = Screen::Repository;
    }

    pub fn open_commit(&mut self, detail: CommitDetail) {
        self.commit = Some(CommitState {
            detail,
            vertical_scroll: 0,
        });
        self.screen = Screen::Commit;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return AppCommand::Quit;
        }

        if self.loading.is_some() {
            return AppCommand::None;
        }

        if self.modal.is_some() {
            return self.handle_modal_key(key);
        }

        if key.code == KeyCode::Char('?')
            && !(self.screen == Screen::Home && self.home.focus == HomeFocus::Search)
        {
            self.modal = Some(Modal::Help);
            return AppCommand::None;
        }

        match self.screen {
            Screen::Home => self.handle_home_key(key),
            Screen::Repository => self.handle_repository_key(key),
            Screen::File => self.handle_file_key(key),
            Screen::Commit => self.handle_commit_key(key),
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> AppCommand {
        match self.modal.as_ref() {
            Some(Modal::RateLimit { .. }) => match key.code {
                KeyCode::Enter | KeyCode::Char('a') => {
                    self.modal = None;
                    AppCommand::Authenticate
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = None;
                    self.pending_retry = None;
                    AppCommand::None
                }
                _ => AppCommand::None,
            },
            Some(Modal::Help | Modal::Error { .. }) => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                    self.modal = None;
                    AppCommand::None
                }
                _ => AppCommand::None,
            },
            None => AppCommand::None,
        }
    }

    fn handle_home_key(&mut self, key: KeyEvent) -> AppCommand {
        if self.home.focus == HomeFocus::Search {
            return self.handle_search_key(key);
        }

        match key.code {
            KeyCode::Char('q') => AppCommand::Quit,
            KeyCode::Char('/') => {
                self.home.focus = HomeFocus::Search;
                AppCommand::None
            }
            KeyCode::Tab | KeyCode::Right => {
                self.home.focus = self.home.focus.next();
                AppCommand::None
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.home.focus = self.home.focus.previous();
                AppCommand::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_home_selection(-1);
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_home_selection(1);
                AppCommand::None
            }
            KeyCode::Enter => self.open_selected_home_item(),
            KeyCode::Char('r') => AppCommand::RefreshHome,
            _ => AppCommand::None,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Enter => {
                let query = self.home.query.trim();
                if query.is_empty() {
                    self.show_error(
                        "Repository",
                        "owner/repo または https://github.com/owner/repo を入力してください",
                    );
                    return AppCommand::None;
                }
                match RepositoryId::from_str(query) {
                    Ok(id) => AppCommand::OpenRepository {
                        id,
                        resume_path: None,
                        resume_screen: HistoryScreen::Code,
                    },
                    Err(error) => {
                        self.show_error("Repository", error.to_string());
                        AppCommand::None
                    }
                }
            }
            KeyCode::Backspace => {
                self.home.query.pop();
                AppCommand::None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.home.query.clear();
                AppCommand::None
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.home.query.push(character);
                AppCommand::None
            }
            KeyCode::Tab | KeyCode::Down => {
                self.home.focus = HomeFocus::History;
                AppCommand::None
            }
            KeyCode::BackTab => {
                self.home.focus = HomeFocus::Recommended;
                AppCommand::None
            }
            KeyCode::Esc => {
                self.home.query.clear();
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_repository_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.code == KeyCode::Char('q') {
            return AppCommand::Quit;
        }

        let Some(repository) = self.repository.as_mut() else {
            self.screen = Screen::Home;
            return AppCommand::None;
        };

        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Home;
                AppCommand::None
            }
            KeyCode::Char('1') => {
                repository.tab = RepositoryTab::Code;
                AppCommand::None
            }
            KeyCode::Char('2') => {
                repository.tab = RepositoryTab::Commits;
                if repository.commits.is_empty() {
                    AppCommand::LoadCommits { page: 1 }
                } else {
                    AppCommand::None
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                repository.tab = RepositoryTab::Code;
                AppCommand::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                repository.tab = RepositoryTab::Commits;
                if repository.commits.is_empty() {
                    AppCommand::LoadCommits { page: 1 }
                } else {
                    AppCommand::None
                }
            }
            _ => match repository.tab {
                RepositoryTab::Code => Self::handle_code_key(repository, key),
                RepositoryTab::Commits => Self::handle_commits_key(repository, key),
            },
        }
    }

    fn handle_code_key(repository: &mut RepositoryState, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                repository.entry_index = repository.entry_index.saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !repository.entries.is_empty() {
                    repository.entry_index =
                        (repository.entry_index + 1).min(repository.entries.len() - 1);
                }
                AppCommand::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                repository.entry_index = 0;
                AppCommand::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                repository.entry_index = repository.entries.len().saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Enter => repository
                .selected_entry()
                .map_or(AppCommand::None, |entry| {
                    if entry.kind.is_directory() {
                        AppCommand::OpenDirectory(entry.path.clone())
                    } else if entry.kind.is_file() {
                        AppCommand::OpenFile(entry.path.clone())
                    } else {
                        AppCommand::None
                    }
                }),
            KeyCode::Backspace => {
                if repository.path.is_empty() {
                    AppCommand::None
                } else {
                    AppCommand::OpenDirectory(repository.parent_path())
                }
            }
            KeyCode::Char('r') => AppCommand::OpenDirectory(repository.path.clone()),
            _ => AppCommand::None,
        }
    }

    fn handle_commits_key(repository: &mut RepositoryState, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                repository.commit_index = repository.commit_index.saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !repository.commits.is_empty() {
                    repository.commit_index =
                        (repository.commit_index + 1).min(repository.commits.len() - 1);
                }
                AppCommand::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                repository.commit_index = 0;
                AppCommand::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                repository.commit_index = repository.commits.len().saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Enter => repository
                .selected_commit()
                .map_or(AppCommand::None, |commit| {
                    AppCommand::OpenCommit(commit.sha.clone())
                }),
            KeyCode::Char('n') | KeyCode::PageDown => AppCommand::LoadCommits {
                page: repository.commit_page.saturating_add(1),
            },
            KeyCode::Char('p') | KeyCode::PageUp => AppCommand::LoadCommits {
                page: repository.commit_page.saturating_sub(1).max(1),
            },
            KeyCode::Char('r') => AppCommand::LoadCommits {
                page: repository.commit_page,
            },
            _ => AppCommand::None,
        }
    }

    fn handle_file_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.code == KeyCode::Char('q') {
            return AppCommand::Quit;
        }
        let Some(file) = self.file.as_mut() else {
            self.screen = Screen::Repository;
            return AppCommand::None;
        };

        match key.code {
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') => {
                self.screen = Screen::Repository;
                AppCommand::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                file.vertical_scroll = file.vertical_scroll.saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                file.vertical_scroll =
                    (file.vertical_scroll + 1).min(file.line_count().saturating_sub(1));
                AppCommand::None
            }
            KeyCode::PageUp => {
                file.vertical_scroll = file.vertical_scroll.saturating_sub(20);
                AppCommand::None
            }
            KeyCode::PageDown => {
                file.vertical_scroll =
                    (file.vertical_scroll + 20).min(file.line_count().saturating_sub(1));
                AppCommand::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                file.vertical_scroll = 0;
                AppCommand::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                file.vertical_scroll = file.line_count().saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                file.horizontal_scroll = file.horizontal_scroll.saturating_sub(4);
                AppCommand::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                file.horizontal_scroll = file.horizontal_scroll.saturating_add(4);
                AppCommand::None
            }
            KeyCode::Char('p') => AppCommand::ExportFile,
            _ => AppCommand::None,
        }
    }

    fn handle_commit_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.code == KeyCode::Char('q') {
            return AppCommand::Quit;
        }
        let Some(commit) = self.commit.as_mut() else {
            self.screen = Screen::Repository;
            return AppCommand::None;
        };

        match key.code {
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') => {
                self.screen = Screen::Repository;
                AppCommand::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                commit.vertical_scroll = commit.vertical_scroll.saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                commit.vertical_scroll = commit.vertical_scroll.saturating_add(1);
                AppCommand::None
            }
            KeyCode::PageUp => {
                commit.vertical_scroll = commit.vertical_scroll.saturating_sub(20);
                AppCommand::None
            }
            KeyCode::PageDown => {
                commit.vertical_scroll = commit.vertical_scroll.saturating_add(20);
                AppCommand::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                commit.vertical_scroll = 0;
                AppCommand::None
            }
            KeyCode::Char('p') => AppCommand::ExportCommit,
            _ => AppCommand::None,
        }
    }

    fn move_home_selection(&mut self, delta: isize) {
        let (index, len) = match self.home.focus {
            HomeFocus::Search => return,
            HomeFocus::History => (&mut self.home.history_index, self.home.history.len()),
            HomeFocus::Featured => (&mut self.home.featured_index, self.home.featured.len()),
            HomeFocus::Recommended => (
                &mut self.home.recommended_index,
                self.home.recommended.len(),
            ),
        };
        if len == 0 {
            *index = 0;
            return;
        }
        if delta.is_negative() {
            *index = index.saturating_sub(delta.unsigned_abs());
        } else {
            *index = (*index + delta as usize).min(len - 1);
        }
    }

    fn open_selected_home_item(&self) -> AppCommand {
        match self.home.focus {
            HomeFocus::Search => AppCommand::None,
            HomeFocus::History => {
                self.home
                    .history
                    .get(self.home.history_index)
                    .map_or(AppCommand::None, |entry| AppCommand::OpenRepository {
                        id: entry.repository.id.clone(),
                        resume_path: entry.last_path.clone(),
                        resume_screen: entry.last_screen,
                    })
            }
            HomeFocus::Featured => {
                self.home
                    .featured
                    .get(self.home.featured_index)
                    .map_or(AppCommand::None, |entry| AppCommand::OpenRepository {
                        id: entry.id.clone(),
                        resume_path: None,
                        resume_screen: HistoryScreen::Code,
                    })
            }
            HomeFocus::Recommended => self
                .home
                .recommended
                .get(self.home.recommended_index)
                .map_or(AppCommand::None, |entry| AppCommand::OpenRepository {
                    id: entry.id.clone(),
                    resume_path: None,
                    resume_screen: HistoryScreen::Code,
                }),
        }
    }
}

fn fallback_featured() -> Vec<RepoCard> {
    [
        (
            "rust-lang/rust",
            "Empowering everyone to build reliable and efficient software.",
            "Rust",
        ),
        ("torvalds/linux", "The Linux kernel source tree.", "C"),
        (
            "astral-sh/uv",
            "An extremely fast Python package and project manager.",
            "Rust",
        ),
        (
            "BurntSushi/ripgrep",
            "Recursively search directories for a regex pattern.",
            "Rust",
        ),
        (
            "sharkdp/bat",
            "A cat clone with syntax highlighting and Git integration.",
            "Rust",
        ),
    ]
    .into_iter()
    .filter_map(|(name, description, language)| {
        name.parse().ok().map(|id| RepoCard {
            id,
            description: Some(description.to_owned()),
            language: Some(language.to_owned()),
            stars: 0,
            updated_at: None,
        })
    })
    .collect()
}

fn fallback_recommended() -> Vec<RepoCard> {
    [
        (
            "ratatui/ratatui",
            "A Rust crate for cooking up terminal user interfaces.",
            "Rust",
        ),
        (
            "gitui-org/gitui",
            "Blazing fast terminal UI for Git.",
            "Rust",
        ),
        (
            "jesseduffield/lazygit",
            "A simple terminal UI for Git commands.",
            "Go",
        ),
        (
            "dandavison/delta",
            "A syntax-highlighting pager for Git and diff output.",
            "Rust",
        ),
        (
            "charmbracelet/gum",
            "A tool for glamorous shell scripts.",
            "Go",
        ),
    ]
    .into_iter()
    .filter_map(|(name, description, language)| {
        name.parse().ok().map(|id| RepoCard {
            id,
            description: Some(description.to_owned()),
            language: Some(language.to_owned()),
            stars: 0,
            updated_at: None,
        })
    })
    .collect()
}
