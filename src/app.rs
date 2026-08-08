use std::{
    str::FromStr,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    icons::Icons,
    model::{
        BlameRange, BranchSummary, CodeSearchResult, CommitDetail, CommitSummary, ContentEntry,
        HistoryEntry, HistoryScreen, IssueSummary, PullRequestSummary, RateLimit, ReleaseSummary,
        RepoCard, Repository, RepositoryId, SymbolLocation, TreeEntry, WorkflowRunSummary,
    },
    symbols,
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
    PullRequests,
    Issues,
    Actions,
    Releases,
}

impl RepositoryTab {
    pub const ALL: [Self; 6] = [
        Self::Code,
        Self::Commits,
        Self::PullRequests,
        Self::Issues,
        Self::Actions,
        Self::Releases,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Code => 0,
            Self::Commits => 1,
            Self::PullRequests => 2,
            Self::Issues => 3,
            Self::Actions => 4,
            Self::Releases => 5,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    #[must_use]
    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTab {
    Code,
    Blame,
    History,
}

impl FileTab {
    fn next(self) -> Self {
        match self {
            Self::Code => Self::Blame,
            Self::Blame => Self::History,
            Self::History => Self::Code,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Code => Self::History,
            Self::Blame => Self::Code,
            Self::History => Self::Blame,
        }
    }
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
    pub selected_ref: String,
    pub path: String,
    pub entries: Vec<ContentEntry>,
    pub entry_index: usize,
    pub tab: RepositoryTab,
    pub commits: Vec<CommitSummary>,
    pub commit_index: usize,
    pub commit_page: u32,
    pub pull_requests: Vec<PullRequestSummary>,
    pub issues: Vec<IssueSummary>,
    pub workflow_runs: Vec<WorkflowRunSummary>,
    pub releases: Vec<ReleaseSummary>,
    pub list_index: usize,
    pub branches: Vec<BranchSummary>,
    pub tree_cache: Option<Vec<TreeEntry>>,
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

    #[must_use]
    pub fn active_list_len(&self) -> usize {
        match self.tab {
            RepositoryTab::Code => self.entries.len(),
            RepositoryTab::Commits => self.commits.len(),
            RepositoryTab::PullRequests => self.pull_requests.len(),
            RepositoryTab::Issues => self.issues.len(),
            RepositoryTab::Actions => self.workflow_runs.len(),
            RepositoryTab::Releases => self.releases.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileState {
    pub path: String,
    pub content: String,
    pub cursor_line: usize,
    pub viewport_top: usize,
    pub horizontal_scroll: usize,
    pub selection_anchor: Option<usize>,
    pub tab: FileTab,
    pub blame: Vec<BlameRange>,
    pub history: Vec<CommitSummary>,
    pub history_index: usize,
    pub history_loaded: bool,
    pub blame_loaded: bool,
}

impl FileState {
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.content.lines().count().max(1)
    }

    #[must_use]
    pub fn selection_range(&self) -> (usize, usize) {
        let anchor = self.selection_anchor.unwrap_or(self.cursor_line);
        (anchor.min(self.cursor_line), anchor.max(self.cursor_line))
    }

    #[must_use]
    pub fn selected_text(&self) -> String {
        let (start, end) = self.selection_range();
        self.content
            .lines()
            .skip(start)
            .take(end.saturating_sub(start) + 1)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[must_use]
    pub fn cursor_line_text(&self) -> &str {
        self.content
            .lines()
            .nth(self.cursor_line)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct CommitState {
    pub detail: CommitDetail,
    pub vertical_scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSearchMode {
    Text,
    Definition,
}

#[derive(Debug, Clone)]
pub enum Modal {
    Help,
    Error {
        title: String,
        message: String,
    },
    RateLimit {
        rate_limit: RateLimit,
    },
    AuthMenu {
        index: usize,
    },
    TokenInput {
        input: String,
        persist: bool,
    },
    BranchPicker {
        query: String,
        branches: Vec<BranchSummary>,
        index: usize,
    },
    RepositorySearch {
        query: String,
        results: Vec<RepoCard>,
        index: usize,
    },
    FileSearch {
        query: String,
        all_files: Vec<String>,
        results: Vec<String>,
        index: usize,
    },
    CodeSearch {
        mode: CodeSearchMode,
        query: String,
        results: Vec<CodeSearchResult>,
        index: usize,
    },
    SymbolPicker {
        query: String,
        all_symbols: Vec<SymbolLocation>,
        results: Vec<SymbolLocation>,
        index: usize,
    },
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
    SearchRepositories(String),
    OpenDirectory(String),
    OpenFile {
        path: String,
        find: Option<String>,
    },
    LoadRepositoryTab(RepositoryTab),
    LoadCommits {
        page: u32,
    },
    OpenCommit(String),
    LoadBranches,
    SwitchBranch(String),
    LoadTreeForSearch,
    SearchCode {
        query: String,
        mode: CodeSearchMode,
    },
    LoadBlame,
    LoadFileHistory,
    AuthenticateCli,
    SetToken {
        token: String,
        persist: bool,
    },
    CopyText(String),
    PasteClipboard,
    ExportFile,
    ExportCommit,
    OpenExternal(String),
}

#[derive(Debug)]
struct StatusMessage {
    text: String,
    created_at: Instant,
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
    status: Option<StatusMessage>,
    pub rate_limit: Option<RateLimit>,
    pub icons: Icons,
    pub authenticated: bool,
    pub auth_user: Option<String>,
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
            auth_user: None,
            pending_retry: None,
        }
    }

    #[must_use]
    pub fn current_repository(&self) -> Option<&Repository> {
        self.repository.as_ref().map(|state| &state.repository)
    }

    #[must_use]
    pub fn current_ref(&self) -> Option<&str> {
        self.repository
            .as_ref()
            .map(|state| state.selected_ref.as_str())
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

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = Some(StatusMessage {
            text: text.into(),
            created_at: Instant::now(),
        });
    }

    pub fn expire_status(&mut self) {
        if self
            .status
            .as_ref()
            .is_some_and(|status| status.created_at.elapsed() >= Duration::from_secs(4))
        {
            self.status = None;
        }
    }

    #[must_use]
    pub fn status_text(&self) -> Option<&str> {
        self.status.as_ref().map(|status| status.text.as_str())
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

    pub fn show_auth_required(&mut self, retry: AppCommand) {
        self.pending_retry = Some(retry);
        self.modal = Some(Modal::AuthMenu { index: 0 });
    }

    pub fn open_repository(
        &mut self,
        repository: Repository,
        path: String,
        entries: Vec<ContentEntry>,
    ) {
        let selected_ref = repository.default_branch.clone();
        self.repository = Some(RepositoryState {
            repository,
            selected_ref,
            path,
            entries,
            entry_index: 0,
            tab: RepositoryTab::Code,
            commits: Vec::new(),
            commit_index: 0,
            commit_page: 1,
            pull_requests: Vec::new(),
            issues: Vec::new(),
            workflow_runs: Vec::new(),
            releases: Vec::new(),
            list_index: 0,
            branches: Vec::new(),
            tree_cache: None,
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

    pub fn switch_branch(&mut self, branch: String, path: String, entries: Vec<ContentEntry>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.selected_ref = branch;
            repository.path = path;
            repository.entries = entries;
            repository.entry_index = 0;
            repository.tab = RepositoryTab::Code;
            repository.commits.clear();
            repository.pull_requests.clear();
            repository.issues.clear();
            repository.workflow_runs.clear();
            repository.releases.clear();
            repository.tree_cache = None;
            repository.list_index = 0;
        }
        self.file = None;
        self.commit = None;
        self.screen = Screen::Repository;
    }

    pub fn open_file(&mut self, path: String, content: String, find: Option<&str>) {
        let mut cursor_line = 0;
        if let Some(needle) = find.filter(|needle| !needle.trim().is_empty()) {
            let needle = needle.to_ascii_lowercase();
            cursor_line = content
                .lines()
                .position(|line| line.to_ascii_lowercase().contains(&needle))
                .unwrap_or(0);
        }
        self.file = Some(FileState {
            path,
            content,
            cursor_line,
            viewport_top: cursor_line.saturating_sub(4),
            horizontal_scroll: 0,
            selection_anchor: None,
            tab: FileTab::Code,
            blame: Vec::new(),
            history: Vec::new(),
            history_index: 0,
            history_loaded: false,
            blame_loaded: false,
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

    pub fn set_pull_requests(&mut self, values: Vec<PullRequestSummary>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.tab = RepositoryTab::PullRequests;
            repository.pull_requests = values;
            repository.list_index = 0;
        }
    }

    pub fn set_issues(&mut self, values: Vec<IssueSummary>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.tab = RepositoryTab::Issues;
            repository.issues = values;
            repository.list_index = 0;
        }
    }

    pub fn set_workflow_runs(&mut self, values: Vec<WorkflowRunSummary>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.tab = RepositoryTab::Actions;
            repository.workflow_runs = values;
            repository.list_index = 0;
        }
    }

    pub fn set_releases(&mut self, values: Vec<ReleaseSummary>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.tab = RepositoryTab::Releases;
            repository.releases = values;
            repository.list_index = 0;
        }
    }

    pub fn open_commit(&mut self, detail: CommitDetail) {
        self.commit = Some(CommitState {
            detail,
            vertical_scroll: 0,
        });
        self.screen = Screen::Commit;
    }

    pub fn set_blame(&mut self, ranges: Vec<BlameRange>) {
        if let Some(file) = self.file.as_mut() {
            file.blame = ranges;
            file.blame_loaded = true;
            file.tab = FileTab::Blame;
        }
    }

    pub fn set_file_history(&mut self, commits: Vec<CommitSummary>) {
        if let Some(file) = self.file.as_mut() {
            file.history = commits;
            file.history_index = 0;
            file.history_loaded = true;
            file.tab = FileTab::History;
        }
    }

    pub fn set_branches(&mut self, branches: Vec<BranchSummary>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.branches = branches.clone();
        }
        self.modal = Some(Modal::BranchPicker {
            query: String::new(),
            branches,
            index: 0,
        });
    }

    pub fn set_tree_and_open_search(&mut self, tree: Vec<TreeEntry>) {
        let files = tree
            .iter()
            .filter(|entry| entry.is_file())
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        if let Some(repository) = self.repository.as_mut() {
            repository.tree_cache = Some(tree);
        }
        self.modal = Some(Modal::FileSearch {
            query: String::new(),
            results: files.iter().take(200).cloned().collect(),
            all_files: files,
            index: 0,
        });
    }

    pub fn open_file_search_from_cache(&mut self) {
        let Some(tree) = self
            .repository
            .as_ref()
            .and_then(|repository| repository.tree_cache.as_ref())
        else {
            return;
        };
        let files = tree
            .iter()
            .filter(|entry| entry.is_file())
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        self.modal = Some(Modal::FileSearch {
            query: String::new(),
            results: files.iter().take(200).cloned().collect(),
            all_files: files,
            index: 0,
        });
    }

    pub fn set_repository_search_results(&mut self, query: String, results: Vec<RepoCard>) {
        self.modal = Some(Modal::RepositorySearch {
            query,
            results,
            index: 0,
        });
    }

    pub fn set_code_search_results(
        &mut self,
        query: String,
        mode: CodeSearchMode,
        results: Vec<CodeSearchResult>,
    ) {
        self.modal = Some(Modal::CodeSearch {
            mode,
            query,
            results,
            index: 0,
        });
    }

    pub fn handle_paste(&mut self, text: String) {
        let text = text.replace('\r', " ").replace('\n', " ");
        match self.modal.as_mut() {
            Some(Modal::TokenInput { input, .. }) => input.push_str(text.trim()),
            Some(Modal::RepositorySearch {
                query,
                results,
                index,
            }) => {
                query.push_str(&text);
                results.clear();
                *index = 0;
            }
            Some(Modal::BranchPicker { query, .. }) => query.push_str(&text),
            Some(Modal::FileSearch {
                query,
                all_files,
                results,
                index,
            }) => {
                query.push_str(&text);
                *results = filter_paths(all_files, query);
                *index = 0;
            }
            Some(Modal::CodeSearch {
                query,
                results,
                index,
                ..
            }) => {
                query.push_str(&text);
                results.clear();
                *index = 0;
            }
            Some(Modal::SymbolPicker {
                query,
                all_symbols,
                results,
                index,
            }) => {
                query.push_str(&text);
                *results = filter_symbols(all_symbols, query);
                *index = 0;
            }
            None if self.screen == Screen::Home && self.home.focus == HomeFocus::Search => {
                self.home.query.push_str(&text);
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return AppCommand::Quit;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('a') {
            self.modal = Some(Modal::AuthMenu { index: 0 });
            return AppCommand::None;
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
        if key.code == KeyCode::Char('a')
            && !(self.screen == Screen::Home && self.home.focus == HomeFocus::Search)
        {
            self.modal = Some(Modal::AuthMenu { index: 0 });
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
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            return AppCommand::PasteClipboard;
        }

        let Some(modal) = self.modal.take() else {
            return AppCommand::None;
        };
        match modal {
            Modal::Help => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => AppCommand::None,
                _ => {
                    self.modal = Some(Modal::Help);
                    AppCommand::None
                }
            },
            Modal::Error { title, message } => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => AppCommand::None,
                _ => {
                    self.modal = Some(Modal::Error { title, message });
                    AppCommand::None
                }
            },
            Modal::RateLimit { rate_limit } => match key.code {
                KeyCode::Enter | KeyCode::Char('a') => {
                    self.modal = Some(Modal::AuthMenu { index: 0 });
                    AppCommand::None
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.pending_retry = None;
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::RateLimit { rate_limit });
                    AppCommand::None
                }
            },
            Modal::AuthMenu { mut index } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => AppCommand::None,
                KeyCode::Up | KeyCode::Char('k') => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::AuthMenu { index });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    index = (index + 1).min(2);
                    self.modal = Some(Modal::AuthMenu { index });
                    AppCommand::None
                }
                KeyCode::Char('1') => AppCommand::AuthenticateCli,
                KeyCode::Char('2') => {
                    self.modal = Some(Modal::TokenInput {
                        input: String::new(),
                        persist: false,
                    });
                    AppCommand::None
                }
                KeyCode::Char('3') => {
                    self.modal = Some(Modal::TokenInput {
                        input: String::new(),
                        persist: true,
                    });
                    AppCommand::None
                }
                KeyCode::Enter => match index {
                    0 => AppCommand::AuthenticateCli,
                    1 => {
                        self.modal = Some(Modal::TokenInput {
                            input: String::new(),
                            persist: false,
                        });
                        AppCommand::None
                    }
                    _ => {
                        self.modal = Some(Modal::TokenInput {
                            input: String::new(),
                            persist: true,
                        });
                        AppCommand::None
                    }
                },
                _ => {
                    self.modal = Some(Modal::AuthMenu { index });
                    AppCommand::None
                }
            },
            Modal::TokenInput { mut input, persist } => match key.code {
                KeyCode::Esc => AppCommand::None,
                KeyCode::Backspace => {
                    input.pop();
                    self.modal = Some(Modal::TokenInput { input, persist });
                    AppCommand::None
                }
                KeyCode::Enter => {
                    let token = input.trim().to_owned();
                    if token.is_empty() {
                        self.modal = Some(Modal::TokenInput { input, persist });
                        AppCommand::None
                    } else {
                        AppCommand::SetToken { token, persist }
                    }
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    input.push(character);
                    self.modal = Some(Modal::TokenInput { input, persist });
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::TokenInput { input, persist });
                    AppCommand::None
                }
            },
            Modal::BranchPicker {
                mut query,
                branches,
                mut index,
            } => {
                let filtered = filter_branches(&branches, &query);
                match key.code {
                    KeyCode::Esc => AppCommand::None,
                    KeyCode::Up | KeyCode::Char('k') => {
                        index = index.saturating_sub(1);
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index,
                        });
                        AppCommand::None
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        index = (index + 1).min(filtered.len().saturating_sub(1));
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index,
                        });
                        AppCommand::None
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index: 0,
                        });
                        AppCommand::None
                    }
                    KeyCode::Enter => filtered.get(index).map_or(AppCommand::None, |branch| {
                        AppCommand::SwitchBranch(branch.name.clone())
                    }),
                    KeyCode::Char(character)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        query.push(character);
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index: 0,
                        });
                        AppCommand::None
                    }
                    _ => {
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index,
                        });
                        AppCommand::None
                    }
                }
            }
            Modal::RepositorySearch {
                mut query,
                mut results,
                mut index,
            } => match key.code {
                KeyCode::Esc => AppCommand::None,
                KeyCode::Up | KeyCode::Char('k') => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    index = (index + 1).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Backspace => {
                    query.pop();
                    results.clear();
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::Enter if !results.is_empty() => {
                    let id = results.get(index).map(|item| item.id.clone());
                    id.map_or(AppCommand::None, |id| AppCommand::OpenRepository {
                        id,
                        resume_path: None,
                        resume_screen: HistoryScreen::Code,
                    })
                }
                KeyCode::Enter => {
                    let search = query.trim().to_owned();
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index,
                    });
                    if search.is_empty() {
                        AppCommand::None
                    } else {
                        AppCommand::SearchRepositories(search)
                    }
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    query.push(character);
                    results.clear();
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
            },
            Modal::FileSearch {
                mut query,
                all_files,
                mut results,
                mut index,
            } => match key.code {
                KeyCode::Esc => AppCommand::None,
                KeyCode::Up | KeyCode::Char('k') => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    index = (index + 1).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Backspace => {
                    query.pop();
                    results = filter_paths(&all_files, &query);
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::Enter => {
                    results
                        .get(index)
                        .map_or(AppCommand::None, |path| AppCommand::OpenFile {
                            path: path.clone(),
                            find: None,
                        })
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    query.push(character);
                    results = filter_paths(&all_files, &query);
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index,
                    });
                    AppCommand::None
                }
            },
            Modal::CodeSearch {
                mode,
                mut query,
                mut results,
                mut index,
            } => match key.code {
                KeyCode::Esc => AppCommand::None,
                KeyCode::Up | KeyCode::Char('k') => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    index = (index + 1).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Backspace => {
                    query.pop();
                    results.clear();
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::Enter if !results.is_empty() => {
                    results
                        .get(index)
                        .map_or(AppCommand::None, |result| AppCommand::OpenFile {
                            path: result.path.clone(),
                            find: Some(query.clone()),
                        })
                }
                KeyCode::Enter => {
                    let search = query.trim().to_owned();
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index,
                    });
                    if search.is_empty() {
                        AppCommand::None
                    } else {
                        AppCommand::SearchCode {
                            query: search,
                            mode,
                        }
                    }
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    query.push(character);
                    results.clear();
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
            },
            Modal::SymbolPicker {
                mut query,
                all_symbols,
                mut results,
                mut index,
            } => match key.code {
                KeyCode::Esc => AppCommand::None,
                KeyCode::Up | KeyCode::Char('k') => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    index = (index + 1).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Backspace => {
                    query.pop();
                    results = filter_symbols(&all_symbols, &query);
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::Enter => {
                    if let Some(symbol) = results.get(index) {
                        if let Some(file) = self.file.as_mut() {
                            file.cursor_line = symbol.line.saturating_sub(1);
                            file.viewport_top = file.cursor_line.saturating_sub(4);
                            file.tab = FileTab::Code;
                        }
                    }
                    AppCommand::None
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    query.push(character);
                    results = filter_symbols(&all_symbols, &query);
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index,
                    });
                    AppCommand::None
                }
            },
        }
    }

    fn handle_home_key(&mut self, key: KeyEvent) -> AppCommand {
        if self.home.focus == HomeFocus::Search {
            return self.handle_home_search_key(key);
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

    fn handle_home_search_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            return AppCommand::PasteClipboard;
        }
        match key.code {
            KeyCode::Enter => {
                let query = self.home.query.trim().to_owned();
                if query.is_empty() {
                    return AppCommand::None;
                }
                if let Ok(id) = RepositoryId::from_str(&query) {
                    AppCommand::OpenRepository {
                        id,
                        resume_path: None,
                        resume_screen: HistoryScreen::Code,
                    }
                } else {
                    self.modal = Some(Modal::RepositorySearch {
                        query: query.clone(),
                        results: Vec::new(),
                        index: 0,
                    });
                    AppCommand::SearchRepositories(query)
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
            KeyCode::Char('B') => AppCommand::LoadBranches,
            KeyCode::Char('f') => {
                if repository.tree_cache.is_some() {
                    self.open_file_search_from_cache();
                    AppCommand::None
                } else {
                    AppCommand::LoadTreeForSearch
                }
            }
            KeyCode::Char('s') | KeyCode::Char('/') => {
                self.modal = Some(Modal::CodeSearch {
                    mode: CodeSearchMode::Text,
                    query: String::new(),
                    results: Vec::new(),
                    index: 0,
                });
                AppCommand::None
            }
            KeyCode::Char('1') => self.select_repository_tab(RepositoryTab::Code),
            KeyCode::Char('2') => self.select_repository_tab(RepositoryTab::Commits),
            KeyCode::Char('3') => self.select_repository_tab(RepositoryTab::PullRequests),
            KeyCode::Char('4') => self.select_repository_tab(RepositoryTab::Issues),
            KeyCode::Char('5') => self.select_repository_tab(RepositoryTab::Actions),
            KeyCode::Char('6') => self.select_repository_tab(RepositoryTab::Releases),
            KeyCode::Left | KeyCode::Char('h') => {
                let tab = repository.tab.previous();
                self.select_repository_tab(tab)
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                let tab = repository.tab.next();
                self.select_repository_tab(tab)
            }
            KeyCode::BackTab => {
                let tab = repository.tab.previous();
                self.select_repository_tab(tab)
            }
            KeyCode::Char('u') | KeyCode::Backspace if repository.tab == RepositoryTab::Code => {
                if repository.path.is_empty() {
                    AppCommand::None
                } else {
                    AppCommand::OpenDirectory(repository.parent_path())
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                move_repository_selection(repository, -1);
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                move_repository_selection(repository, 1);
                AppCommand::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                set_repository_selection(repository, 0);
                AppCommand::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                let last = repository.active_list_len().saturating_sub(1);
                set_repository_selection(repository, last);
                AppCommand::None
            }
            KeyCode::Enter => enter_repository_selection(repository),
            KeyCode::PageDown if repository.tab == RepositoryTab::Commits => {
                AppCommand::LoadCommits {
                    page: repository.commit_page.saturating_add(1),
                }
            }
            KeyCode::PageUp if repository.tab == RepositoryTab::Commits => {
                AppCommand::LoadCommits {
                    page: repository.commit_page.saturating_sub(1).max(1),
                }
            }
            KeyCode::Char('r') => self.reload_repository_tab(),
            _ => AppCommand::None,
        }
    }

    fn select_repository_tab(&mut self, tab: RepositoryTab) -> AppCommand {
        let Some(repository) = self.repository.as_mut() else {
            return AppCommand::None;
        };
        repository.tab = tab;
        repository.list_index = 0;
        let loaded = match tab {
            RepositoryTab::Code => true,
            RepositoryTab::Commits => !repository.commits.is_empty(),
            RepositoryTab::PullRequests => !repository.pull_requests.is_empty(),
            RepositoryTab::Issues => !repository.issues.is_empty(),
            RepositoryTab::Actions => !repository.workflow_runs.is_empty(),
            RepositoryTab::Releases => !repository.releases.is_empty(),
        };
        if loaded {
            AppCommand::None
        } else {
            AppCommand::LoadRepositoryTab(tab)
        }
    }

    fn reload_repository_tab(&self) -> AppCommand {
        self.repository
            .as_ref()
            .map_or(AppCommand::None, |repository| match repository.tab {
                RepositoryTab::Code => AppCommand::OpenDirectory(repository.path.clone()),
                RepositoryTab::Commits => AppCommand::LoadCommits {
                    page: repository.commit_page,
                },
                tab => AppCommand::LoadRepositoryTab(tab),
            })
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
            KeyCode::Tab => {
                let next = file.tab.next();
                file.tab = next;
                self.ensure_file_tab_loaded(next)
            }
            KeyCode::BackTab => {
                let previous = file.tab.previous();
                file.tab = previous;
                self.ensure_file_tab_loaded(previous)
            }
            KeyCode::Char('1') => {
                file.tab = FileTab::Code;
                AppCommand::None
            }
            KeyCode::Char('2') => {
                file.tab = FileTab::Blame;
                self.ensure_file_tab_loaded(FileTab::Blame)
            }
            KeyCode::Char('3') => {
                file.tab = FileTab::History;
                self.ensure_file_tab_loaded(FileTab::History)
            }
            KeyCode::Char('@') => {
                let all_symbols = symbols::extract_symbols(&file.path, &file.content);
                let results = all_symbols.clone();
                self.modal = Some(Modal::SymbolPicker {
                    query: String::new(),
                    all_symbols,
                    results,
                    index: 0,
                });
                AppCommand::None
            }
            KeyCode::Char('d') => {
                let seed = identifier_from_line(file.cursor_line_text());
                self.modal = Some(Modal::CodeSearch {
                    mode: CodeSearchMode::Definition,
                    query: seed,
                    results: Vec::new(),
                    index: 0,
                });
                AppCommand::None
            }
            KeyCode::Char('v') if file.tab != FileTab::History => {
                file.selection_anchor = if file.selection_anchor.is_some() {
                    None
                } else {
                    Some(file.cursor_line)
                };
                AppCommand::None
            }
            KeyCode::Char('y') if file.tab != FileTab::History => {
                AppCommand::CopyText(file.selected_text())
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if file.tab == FileTab::History {
                    file.history_index = file.history_index.saturating_sub(1);
                } else {
                    file.cursor_line = file.cursor_line.saturating_sub(1);
                    keep_cursor_visible(file);
                }
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if file.tab == FileTab::History {
                    if !file.history.is_empty() {
                        file.history_index = (file.history_index + 1).min(file.history.len() - 1);
                    }
                } else {
                    file.cursor_line =
                        (file.cursor_line + 1).min(file.line_count().saturating_sub(1));
                    keep_cursor_visible(file);
                }
                AppCommand::None
            }
            KeyCode::PageUp => {
                if file.tab == FileTab::History {
                    file.history_index = file.history_index.saturating_sub(15);
                } else {
                    file.cursor_line = file.cursor_line.saturating_sub(20);
                    keep_cursor_visible(file);
                }
                AppCommand::None
            }
            KeyCode::PageDown => {
                if file.tab == FileTab::History {
                    file.history_index =
                        (file.history_index + 15).min(file.history.len().saturating_sub(1));
                } else {
                    file.cursor_line =
                        (file.cursor_line + 20).min(file.line_count().saturating_sub(1));
                    keep_cursor_visible(file);
                }
                AppCommand::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                if file.tab == FileTab::History {
                    file.history_index = 0;
                } else {
                    file.cursor_line = 0;
                    file.viewport_top = 0;
                }
                AppCommand::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                if file.tab == FileTab::History {
                    file.history_index = file.history.len().saturating_sub(1);
                } else {
                    file.cursor_line = file.line_count().saturating_sub(1);
                    keep_cursor_visible(file);
                }
                AppCommand::None
            }
            KeyCode::Left | KeyCode::Char('h') if file.tab != FileTab::History => {
                file.horizontal_scroll = file.horizontal_scroll.saturating_sub(4);
                AppCommand::None
            }
            KeyCode::Right | KeyCode::Char('l') if file.tab != FileTab::History => {
                file.horizontal_scroll = file.horizontal_scroll.saturating_add(4);
                AppCommand::None
            }
            KeyCode::Enter if file.tab == FileTab::History => file
                .history
                .get(file.history_index)
                .map_or(AppCommand::None, |commit| {
                    AppCommand::OpenCommit(commit.sha.clone())
                }),
            KeyCode::Enter if file.tab == FileTab::Blame => {
                blame_at_line(&file.blame, file.cursor_line + 1).map_or(AppCommand::None, |range| {
                    AppCommand::OpenCommit(range.commit_sha.clone())
                })
            }
            KeyCode::Char('p') => AppCommand::ExportFile,
            _ => AppCommand::None,
        }
    }

    fn ensure_file_tab_loaded(&self, tab: FileTab) -> AppCommand {
        let Some(file) = self.file.as_ref() else {
            return AppCommand::None;
        };
        match tab {
            FileTab::Code => AppCommand::None,
            FileTab::Blame if !file.blame_loaded => AppCommand::LoadBlame,
            FileTab::History if !file.history_loaded => AppCommand::LoadFileHistory,
            FileTab::Blame | FileTab::History => AppCommand::None,
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
            KeyCode::Char('y') => AppCommand::CopyText(commit.detail.summary.sha.clone()),
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
        move_index(index, len, delta);
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

fn enter_repository_selection(repository: &RepositoryState) -> AppCommand {
    match repository.tab {
        RepositoryTab::Code => repository
            .selected_entry()
            .map_or(AppCommand::None, |entry| {
                if entry.kind.is_directory() {
                    AppCommand::OpenDirectory(entry.path.clone())
                } else if entry.kind.is_file() {
                    AppCommand::OpenFile {
                        path: entry.path.clone(),
                        find: None,
                    }
                } else {
                    AppCommand::None
                }
            }),
        RepositoryTab::Commits => repository
            .selected_commit()
            .map_or(AppCommand::None, |commit| {
                AppCommand::OpenCommit(commit.sha.clone())
            }),
        RepositoryTab::PullRequests => repository
            .pull_requests
            .get(repository.list_index)
            .map_or(AppCommand::None, |item| {
                AppCommand::OpenExternal(item.html_url.clone())
            }),
        RepositoryTab::Issues => repository
            .issues
            .get(repository.list_index)
            .map_or(AppCommand::None, |item| {
                AppCommand::OpenExternal(item.html_url.clone())
            }),
        RepositoryTab::Actions => repository
            .workflow_runs
            .get(repository.list_index)
            .map_or(AppCommand::None, |item| {
                AppCommand::OpenExternal(item.html_url.clone())
            }),
        RepositoryTab::Releases => repository
            .releases
            .get(repository.list_index)
            .map_or(AppCommand::None, |item| {
                AppCommand::OpenExternal(item.html_url.clone())
            }),
    }
}

fn move_repository_selection(repository: &mut RepositoryState, delta: isize) {
    match repository.tab {
        RepositoryTab::Code => {
            move_index(&mut repository.entry_index, repository.entries.len(), delta)
        }
        RepositoryTab::Commits => move_index(
            &mut repository.commit_index,
            repository.commits.len(),
            delta,
        ),
        RepositoryTab::PullRequests
        | RepositoryTab::Issues
        | RepositoryTab::Actions
        | RepositoryTab::Releases => {
            let len = repository.active_list_len();
            move_index(&mut repository.list_index, len, delta);
        }
    }
}

fn set_repository_selection(repository: &mut RepositoryState, value: usize) {
    match repository.tab {
        RepositoryTab::Code => {
            repository.entry_index = value.min(repository.entries.len().saturating_sub(1))
        }
        RepositoryTab::Commits => {
            repository.commit_index = value.min(repository.commits.len().saturating_sub(1))
        }
        RepositoryTab::PullRequests
        | RepositoryTab::Issues
        | RepositoryTab::Actions
        | RepositoryTab::Releases => {
            repository.list_index = value.min(repository.active_list_len().saturating_sub(1));
        }
    }
}

fn move_index(index: &mut usize, len: usize, delta: isize) {
    if len == 0 {
        *index = 0;
    } else if delta.is_negative() {
        *index = index.saturating_sub(delta.unsigned_abs());
    } else {
        *index = (*index + delta as usize).min(len - 1);
    }
}

fn keep_cursor_visible(file: &mut FileState) {
    const VIEWPORT_HINT: usize = 24;
    if file.cursor_line < file.viewport_top {
        file.viewport_top = file.cursor_line;
    } else if file.cursor_line >= file.viewport_top + VIEWPORT_HINT {
        file.viewport_top = file.cursor_line.saturating_sub(VIEWPORT_HINT - 1);
    }
}

fn blame_at_line(ranges: &[BlameRange], line: usize) -> Option<&BlameRange> {
    ranges
        .iter()
        .find(|range| line >= range.starting_line && line <= range.ending_line)
}

fn filter_paths(all: &[String], query: &str) -> Vec<String> {
    let query = query.trim().to_ascii_lowercase();
    let mut scored = all
        .iter()
        .filter_map(|path| {
            fuzzy_score(&path.to_ascii_lowercase(), &query).map(|score| (score, path))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|(score, path)| (*score, path.len()));
    scored
        .into_iter()
        .take(200)
        .map(|(_, path)| path.clone())
        .collect()
}

fn filter_branches(all: &[BranchSummary], query: &str) -> Vec<BranchSummary> {
    let query = query.trim().to_ascii_lowercase();
    all.iter()
        .filter(|branch| query.is_empty() || branch.name.to_ascii_lowercase().contains(&query))
        .cloned()
        .collect()
}

fn filter_symbols(all: &[SymbolLocation], query: &str) -> Vec<SymbolLocation> {
    let query = query.trim().to_ascii_lowercase();
    all.iter()
        .filter(|symbol| query.is_empty() || symbol.name.to_ascii_lowercase().contains(&query))
        .cloned()
        .collect()
}

fn fuzzy_score(value: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(value.len());
    }
    if let Some(index) = value.find(query) {
        return Some(index);
    }
    let mut cursor = 0usize;
    let mut score = 1000usize;
    for ch in query.chars() {
        let remainder = value.get(cursor..)?;
        let relative = remainder.find(ch)?;
        cursor += relative + ch.len_utf8();
        score += relative;
    }
    Some(score)
}

fn identifier_from_line(line: &str) -> String {
    line.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .filter(|token| token.len() >= 3)
        .filter(|token| {
            !matches!(
                *token,
                "pub"
                    | "let"
                    | "mut"
                    | "self"
                    | "return"
                    | "const"
                    | "static"
                    | "impl"
                    | "struct"
                    | "class"
                    | "function"
                    | "async"
                    | "await"
            )
        })
        .max_by_key(|token| token.len())
        .unwrap_or_default()
        .to_owned()
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
