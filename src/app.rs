use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    diff::{DiffKind, parse_patch},
    icons::Icons,
    language::detect_language,
    model::{
        BlameRange, BranchSummary, CodeSearchResult, Comment, CommitDetail, CommitSummary,
        ContentEntry, ContentKind, HistoryEntry, HistoryScreen, IssueDetail, IssueSummary,
        OpenClosedFilter, PullRequestDetail, PullRequestSummary, RateLimit, ReleaseDetail,
        ReleaseSummary, RepoCard, Repository, RepositoryId, SymbolLocation, TreeEntry,
        WorkflowRunDetail, WorkflowRunSummary,
    },
    settings::Settings,
    symbols,
    theme::Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Repository,
    File,
    Commit,
    Detail,
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
    pub pull_request_filter: OpenClosedFilter,
    pub pull_requests_loaded: bool,
    pub issues: Vec<IssueSummary>,
    pub issue_filter: OpenClosedFilter,
    pub issues_loaded: bool,
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
    pub last_find: Option<String>,
    pub find_matches: Vec<usize>,
    pub find_index: usize,
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
    pub cursor_line: usize,
    pub viewport_top: usize,
    pub horizontal_scroll: usize,
    pub selection_anchor: Option<usize>,
}

impl CommitState {
    #[must_use]
    pub fn text(&self) -> String {
        commit_text(&self.detail)
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.text().lines().count().max(1)
    }

    #[must_use]
    pub fn selected_text(&self) -> String {
        selected_lines(&self.text(), self.selection_anchor, self.cursor_line)
    }
}

#[derive(Debug, Clone)]
pub enum DetailDocument {
    PullRequest(PullRequestDetail),
    Issue(IssueDetail),
    WorkflowRun(WorkflowRunDetail),
    Release(ReleaseDetail),
}

impl DetailDocument {
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Self::PullRequest(value) => {
                format!("PR #{} · {}", value.summary.number, value.summary.title)
            }
            Self::Issue(value) => {
                format!("Issue #{} · {}", value.summary.number, value.summary.title)
            }
            Self::WorkflowRun(value) => format!("Actions · {}", value.summary.name),
            Self::Release(value) => format!("Release · {}", value.summary.tag_name),
        }
    }

    #[must_use]
    pub fn html_url(&self) -> &str {
        match self {
            Self::PullRequest(value) => &value.summary.html_url,
            Self::Issue(value) => &value.summary.html_url,
            Self::WorkflowRun(value) => &value.summary.html_url,
            Self::Release(value) => &value.summary.html_url,
        }
    }

    #[must_use]
    pub fn text(&self) -> String {
        detail_text(self)
    }
}

#[derive(Debug, Clone)]
pub struct DetailState {
    pub document: DetailDocument,
    pub cursor_line: usize,
    pub viewport_top: usize,
    pub horizontal_scroll: usize,
    pub selection_anchor: Option<usize>,
}

impl DetailState {
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.document.text().lines().count().max(1)
    }

    #[must_use]
    pub fn selected_text(&self) -> String {
        selected_lines(
            &self.document.text(),
            self.selection_anchor,
            self.cursor_line,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSearchMode {
    Text,
    Definition,
}

#[derive(Debug, Clone)]
pub enum Modal {
    Help,
    ConfirmClearHistory,
    CacheManager {
        lines: Vec<String>,
    },
    ConfirmClearCache {
        lines: Vec<String>,
    },
    Settings {
        index: usize,
    },
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
    FindInFile {
        query: String,
        matches: Vec<usize>,
        index: usize,
    },
}

#[derive(Debug, Clone)]
pub enum AppCommand {
    None,
    Quit,
    ForceRefresh(Box<AppCommand>),
    ShowCacheManager,
    ClearCache,
    OpenRepository {
        id: RepositoryId,
        resume_path: Option<String>,
        resume_screen: HistoryScreen,
    },
    RefreshHome,
    DeleteHistory(RepositoryId),
    ClearHistory,
    SearchRepositories(String),
    OpenDirectory(String),
    OpenFile {
        path: String,
        find: Option<String>,
        line: Option<usize>,
        definition: bool,
    },
    LoadRepositoryTab(RepositoryTab),
    LoadCommits {
        page: u32,
    },
    OpenCommit(String),
    OpenPullRequest(u64),
    OpenIssue(u64),
    OpenWorkflowRun(u64),
    OpenRelease(u64),
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
    PersistSettings,
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
    pub detail: Option<DetailState>,
    pub modal: Option<Modal>,
    pub loading: Option<String>,
    status: Option<StatusMessage>,
    cache_status: Option<String>,
    pub rate_limit: Option<RateLimit>,
    pub icons: Icons,
    pub settings: Settings,
    pub authenticated: bool,
    pub auth_user: Option<String>,
    pub pending_retry: Option<AppCommand>,
}

impl App {
    #[must_use]
    pub fn new(
        history: Vec<HistoryEntry>,
        icons: Icons,
        authenticated: bool,
        settings: Settings,
    ) -> Self {
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
            detail: None,
            modal: None,
            loading: None,
            status: None,
            cache_status: None,
            rate_limit: None,
            icons,
            settings,
            authenticated,
            auth_user: None,
            pending_retry: None,
        }
    }

    #[must_use]
    pub fn theme(&self) -> Theme {
        self.settings.theme.palette()
    }

    pub fn toggle_theme(&mut self) {
        self.settings.theme = self.settings.theme.toggled();
        self.set_status(format!("Theme: {}", self.settings.theme.label()));
    }

    pub fn toggle_footer_mode(&mut self) {
        self.settings.footer_mode = self.settings.footer_mode.toggled();
        self.set_status(format!(
            "Footer key hints: {}",
            self.settings.footer_mode.label()
        ));
    }

    pub fn toggle_context_wrap(&mut self) {
        match self.screen {
            Screen::File => {
                self.settings.wrap_code = !self.settings.wrap_code;
                self.set_status(if self.settings.wrap_code {
                    "Source wrapping enabled"
                } else {
                    "Source wrapping disabled"
                });
            }
            Screen::Commit | Screen::Detail => {
                self.settings.wrap_diff = !self.settings.wrap_diff;
                self.set_status(if self.settings.wrap_diff {
                    "Detail/diff wrapping enabled"
                } else {
                    "Detail/diff wrapping disabled"
                });
            }
            Screen::Home | Screen::Repository => {}
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

    pub fn set_cache_status(&mut self, text: Option<String>) {
        self.cache_status = text;
    }

    #[must_use]
    pub fn cache_status_text(&self) -> Option<&str> {
        self.cache_status.as_deref()
    }

    pub fn show_cache_manager(&mut self, lines: Vec<String>) {
        self.modal = Some(Modal::CacheManager { lines });
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
        let entries = with_parent_entry(&path, entries);
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
            pull_request_filter: OpenClosedFilter::Open,
            pull_requests_loaded: false,
            issues: Vec::new(),
            issue_filter: OpenClosedFilter::Open,
            issues_loaded: false,
            workflow_runs: Vec::new(),
            releases: Vec::new(),
            list_index: 0,
            branches: Vec::new(),
            tree_cache: None,
        });
        self.file = None;
        self.commit = None;
        self.detail = None;
        self.screen = Screen::Repository;
    }

    pub fn set_directory(&mut self, path: String, entries: Vec<ContentEntry>) {
        let entries = with_parent_entry(&path, entries);
        if let Some(repository) = self.repository.as_mut() {
            repository.path = path;
            repository.entries = entries;
            repository.entry_index = 0;
            repository.tab = RepositoryTab::Code;
        }
        self.screen = Screen::Repository;
    }

    pub fn switch_branch(&mut self, branch: String, path: String, entries: Vec<ContentEntry>) {
        let entries = with_parent_entry(&path, entries);
        if let Some(repository) = self.repository.as_mut() {
            repository.selected_ref = branch;
            repository.path = path;
            repository.entries = entries;
            repository.entry_index = 0;
            repository.tab = RepositoryTab::Code;
            repository.commits.clear();
            repository.pull_requests.clear();
            repository.pull_requests_loaded = false;
            repository.issues.clear();
            repository.issues_loaded = false;
            repository.workflow_runs.clear();
            repository.releases.clear();
            repository.tree_cache = None;
            repository.list_index = 0;
        }
        self.file = None;
        self.commit = None;
        self.detail = None;
        self.screen = Screen::Repository;
    }

    pub fn open_file(
        &mut self,
        path: String,
        content: String,
        find: Option<&str>,
        line: Option<usize>,
    ) {
        let find_query = find
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(str::to_owned);
        let find_matches = find_query
            .as_deref()
            .map(|query| find_lines(&content, query))
            .unwrap_or_default();
        let explicit_line = line
            .and_then(|line| line.checked_sub(1))
            .map(|line| line.min(content.lines().count().saturating_sub(1)));
        let cursor_line = explicit_line
            .or_else(|| find_matches.first().copied())
            .unwrap_or(0);
        let find_index = find_matches
            .iter()
            .position(|matched_line| *matched_line == cursor_line)
            .unwrap_or(0);
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
            last_find: find_query,
            find_matches,
            find_index,
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

    pub fn set_pull_requests(&mut self, filter: OpenClosedFilter, values: Vec<PullRequestSummary>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.tab = RepositoryTab::PullRequests;
            repository.pull_request_filter = filter;
            repository.pull_requests = values;
            repository.pull_requests_loaded = true;
            repository.list_index = 0;
        }
    }

    pub fn set_issues(&mut self, filter: OpenClosedFilter, values: Vec<IssueSummary>) {
        if let Some(repository) = self.repository.as_mut() {
            repository.tab = RepositoryTab::Issues;
            repository.issue_filter = filter;
            repository.issues = values;
            repository.issues_loaded = true;
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
            cursor_line: 0,
            viewport_top: 0,
            horizontal_scroll: 0,
            selection_anchor: None,
        });
        self.screen = Screen::Commit;
    }

    pub fn open_detail(&mut self, document: DetailDocument) {
        self.detail = Some(DetailState {
            document,
            cursor_line: 0,
            viewport_top: 0,
            horizontal_scroll: 0,
            selection_anchor: None,
        });
        self.screen = Screen::Detail;
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

    pub fn set_repository_search_results(&mut self, query: String, mut results: Vec<RepoCard>) {
        rank_repository_results(&query, &mut results);
        if results.is_empty() {
            self.set_status(format!("No repositories matched \"{query}\""));
        } else {
            self.set_status(format!("{} repository matches", results.len()));
        }
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
        match (mode, results.is_empty()) {
            (CodeSearchMode::Definition, true) => self.set_status(format!(
                "No definition found in the source files scanned for \"{query}\""
            )),
            (CodeSearchMode::Text, true) => self.set_status(format!(
                "No match found in the source files scanned for \"{query}\""
            )),
            (CodeSearchMode::Definition, false) => {
                self.set_status(format!("{} definition candidates", results.len()))
            }
            (CodeSearchMode::Text, false) => {
                self.set_status(format!("{} code matches", results.len()))
            }
        }
        self.modal = Some(Modal::CodeSearch {
            mode,
            query,
            results,
            index: 0,
        });
    }

    fn open_find_in_file(&mut self) {
        let Some(file) = self.file.as_ref() else {
            return;
        };
        let query = file.last_find.clone().unwrap_or_default();
        let matches = find_lines(&file.content, &query);
        let index = matches
            .iter()
            .position(|line| *line >= file.cursor_line)
            .unwrap_or(0);
        self.modal = Some(Modal::FindInFile {
            query,
            matches,
            index,
        });
    }

    fn apply_find_in_file(&mut self, query: String, matches: Vec<usize>, index: usize) {
        let query = query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        let status = {
            let Some(file) = self.file.as_mut() else {
                return;
            };
            file.last_find = Some(query.clone());
            file.find_matches = matches;
            file.find_index = index.min(file.find_matches.len().saturating_sub(1));
            if let Some(line) = file.find_matches.get(file.find_index).copied() {
                file.cursor_line = line;
                file.viewport_top = line.saturating_sub(4);
                file.tab = FileTab::Code;
                format!(
                    "Find: {} · match {}/{}",
                    query,
                    file.find_index + 1,
                    file.find_matches.len()
                )
            } else {
                format!("Find: no matches for {query}")
            }
        };
        self.set_status(status);
    }

    fn repeat_find_in_file(&mut self, reverse: bool) {
        if self
            .file
            .as_ref()
            .is_none_or(|file| file.find_matches.is_empty())
        {
            self.open_find_in_file();
            return;
        }
        let status = {
            let file = self.file.as_mut().expect("file state checked above");
            file.find_index = if reverse {
                file.find_index
                    .checked_sub(1)
                    .unwrap_or(file.find_matches.len().saturating_sub(1))
            } else {
                (file.find_index + 1) % file.find_matches.len()
            };
            let line = file.find_matches[file.find_index];
            file.cursor_line = line;
            file.viewport_top = line.saturating_sub(4);
            file.tab = FileTab::Code;
            format!(
                "Find: {} · match {}/{}",
                file.last_find.as_deref().unwrap_or_default(),
                file.find_index + 1,
                file.find_matches.len()
            )
        };
        self.set_status(status);
    }

    pub fn handle_paste(&mut self, text: String) {
        let text = text.replace(['\r', '\n'], " ");
        let file_content = self.file.as_ref().map(|file| file.content.clone());
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
            Some(Modal::FindInFile {
                query,
                matches,
                index,
            }) => {
                query.push_str(&text);
                *matches = file_content
                    .as_deref()
                    .map(|content| find_lines(content, query))
                    .unwrap_or_default();
                *index = 0;
            }
            None if self.screen == Screen::Home && self.home.focus == HomeFocus::Search => {
                self.home.query.push_str(&text);
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
            return AppCommand::Quit;
        }
        if self.loading.is_some() {
            return AppCommand::None;
        }
        if key.code == KeyCode::F(1) {
            self.modal = Some(Modal::Help);
            return AppCommand::None;
        }
        if key.code == KeyCode::F(2) {
            self.modal = Some(Modal::AuthMenu { index: 0 });
            return AppCommand::None;
        }
        if key.code == KeyCode::F(10) {
            self.toggle_footer_mode();
            return AppCommand::PersistSettings;
        }
        if self.modal.is_some() {
            return self.handle_modal_key(key);
        }

        if key.code == KeyCode::F(8) {
            return AppCommand::ShowCacheManager;
        }

        if key.code == KeyCode::Char('?')
            && !(self.screen == Screen::Home && self.home.focus == HomeFocus::Search)
        {
            self.modal = Some(Modal::Help);
            return AppCommand::None;
        }

        if key.code == KeyCode::Char('c')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT)
            && !(self.screen == Screen::Home && self.home.focus == HomeFocus::Search)
        {
            return AppCommand::ShowCacheManager;
        }

        if key.modifiers.contains(KeyModifiers::SHIFT)
            && matches!(key.code, KeyCode::Char('A') | KeyCode::Char('a'))
            && matches!(self.screen, Screen::File | Screen::Commit | Screen::Detail)
        {
            match self.screen {
                Screen::File => {
                    if let Some(file) = self.file.as_mut()
                        && file.tab != FileTab::History
                    {
                        let last = file.line_count().saturating_sub(1);
                        file.selection_anchor = Some(0);
                        file.cursor_line = last;
                        keep_cursor_visible(file);
                    }
                }
                Screen::Commit => {
                    if let Some(commit) = self.commit.as_mut() {
                        let last = commit.line_count().saturating_sub(1);
                        commit.selection_anchor = Some(0);
                        commit.cursor_line = last;
                        keep_reader_cursor_visible(commit.cursor_line, &mut commit.viewport_top);
                    }
                }
                Screen::Detail => {
                    if let Some(detail) = self.detail.as_mut() {
                        let last = detail.line_count().saturating_sub(1);
                        detail.selection_anchor = Some(0);
                        detail.cursor_line = last;
                        keep_reader_cursor_visible(detail.cursor_line, &mut detail.viewport_top);
                    }
                }
                Screen::Home | Screen::Repository => {}
            }
            return AppCommand::None;
        }

        if key.modifiers.contains(KeyModifiers::SHIFT)
            && matches!(key.code, KeyCode::Char('C') | KeyCode::Char('c'))
            && matches!(self.screen, Screen::File | Screen::Commit | Screen::Detail)
        {
            return match self.screen {
                Screen::File => self.file.as_ref().map_or(AppCommand::None, |file| {
                    if file.tab == FileTab::History {
                        AppCommand::None
                    } else {
                        AppCommand::CopyText(file.selected_text())
                    }
                }),
                Screen::Commit => self.commit.as_ref().map_or(AppCommand::None, |commit| {
                    AppCommand::CopyText(commit.selected_text())
                }),
                Screen::Detail => self.detail.as_ref().map_or(AppCommand::None, |detail| {
                    AppCommand::CopyText(detail.selected_text())
                }),
                Screen::Home | Screen::Repository => AppCommand::None,
            };
        }

        if key.code == KeyCode::Char('a')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT)
            && !(self.screen == Screen::Home && self.home.focus == HomeFocus::Search)
        {
            self.modal = Some(Modal::AuthMenu { index: 0 });
            return AppCommand::None;
        }
        if key.code == KeyCode::Char(',') {
            self.modal = Some(Modal::Settings { index: 0 });
            return AppCommand::None;
        }
        if key.code == KeyCode::Char('T') {
            self.toggle_theme();
            return AppCommand::PersistSettings;
        }
        if key.code == KeyCode::Char('w')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT)
            && matches!(self.screen, Screen::File | Screen::Commit | Screen::Detail)
        {
            self.toggle_context_wrap();
            return AppCommand::PersistSettings;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('a') {
            match self.screen {
                Screen::File => {
                    if let Some(file) = self.file.as_mut()
                        && file.tab != FileTab::History
                    {
                        let last = file.line_count().saturating_sub(1);
                        file.selection_anchor = Some(0);
                        file.cursor_line = last;
                        keep_cursor_visible(file);
                    }
                }
                Screen::Commit => {
                    if let Some(commit) = self.commit.as_mut() {
                        let last = commit.line_count().saturating_sub(1);
                        commit.selection_anchor = Some(0);
                        commit.cursor_line = last;
                        keep_reader_cursor_visible(commit.cursor_line, &mut commit.viewport_top);
                    }
                }
                Screen::Detail => {
                    if let Some(detail) = self.detail.as_mut() {
                        let last = detail.line_count().saturating_sub(1);
                        detail.selection_anchor = Some(0);
                        detail.cursor_line = last;
                        keep_reader_cursor_visible(detail.cursor_line, &mut detail.viewport_top);
                    }
                }
                Screen::Home | Screen::Repository => {}
            }
            return AppCommand::None;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return match self.screen {
                Screen::File => self.file.as_ref().map_or(AppCommand::None, |file| {
                    AppCommand::CopyText(file.selected_text())
                }),
                Screen::Commit => self.commit.as_ref().map_or(AppCommand::None, |commit| {
                    AppCommand::CopyText(commit.selected_text())
                }),
                Screen::Detail => self.detail.as_ref().map_or(AppCommand::None, |detail| {
                    AppCommand::CopyText(detail.selected_text())
                }),
                Screen::Home | Screen::Repository => AppCommand::None,
            };
        }

        match self.screen {
            Screen::Home => self.handle_home_key(key),
            Screen::Repository => self.handle_repository_key(key),
            Screen::File => self.handle_file_key(key),
            Screen::Commit => self.handle_commit_key(key),
            Screen::Detail => self.handle_detail_key(key),
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
            Modal::ConfirmClearHistory => match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    AppCommand::ClearHistory
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') => {
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::ConfirmClearHistory);
                    AppCommand::None
                }
            },
            Modal::CacheManager { lines } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => AppCommand::None,
                KeyCode::Char('d') | KeyCode::Delete => {
                    self.modal = Some(Modal::ConfirmClearCache { lines });
                    AppCommand::None
                }
                KeyCode::Char('r') | KeyCode::F(5) => AppCommand::ShowCacheManager,
                _ => {
                    self.modal = Some(Modal::CacheManager { lines });
                    AppCommand::None
                }
            },
            Modal::ConfirmClearCache { lines } => match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => AppCommand::ClearCache,
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') => {
                    self.modal = Some(Modal::CacheManager { lines });
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::ConfirmClearCache { lines });
                    AppCommand::None
                }
            },
            Modal::Settings { mut index } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => AppCommand::None,
                KeyCode::Up | KeyCode::BackTab => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::Settings { index });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Tab => {
                    index = (index + 1).min(3);
                    self.modal = Some(Modal::Settings { index });
                    AppCommand::None
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                    match index {
                        0 => self.settings.theme = self.settings.theme.toggled(),
                        1 => self.settings.wrap_code = !self.settings.wrap_code,
                        2 => self.settings.wrap_diff = !self.settings.wrap_diff,
                        _ => self.settings.footer_mode = self.settings.footer_mode.toggled(),
                    }
                    self.modal = Some(Modal::Settings { index });
                    AppCommand::PersistSettings
                }
                _ => {
                    self.modal = Some(Modal::Settings { index });
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
                KeyCode::Up | KeyCode::BackTab => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::AuthMenu { index });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Tab => {
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
                KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.modal = Some(Modal::TokenInput { input, persist });
                    AppCommand::PasteClipboard
                }
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
                    KeyCode::Up | KeyCode::BackTab => {
                        index = index.saturating_sub(1);
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index,
                        });
                        AppCommand::None
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        index = (index + 1).min(filtered.len().saturating_sub(1));
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index,
                        });
                        AppCommand::None
                    }
                    KeyCode::PageUp => {
                        index = index.saturating_sub(15);
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index,
                        });
                        AppCommand::None
                    }
                    KeyCode::PageDown => {
                        index = (index + 15).min(filtered.len().saturating_sub(1));
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index,
                        });
                        AppCommand::None
                    }
                    KeyCode::Home => {
                        self.modal = Some(Modal::BranchPicker {
                            query,
                            branches,
                            index: 0,
                        });
                        AppCommand::None
                    }
                    KeyCode::End => {
                        index = filtered.len().saturating_sub(1);
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
                KeyCode::Up | KeyCode::BackTab => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Tab => {
                    index = (index + 1).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageUp => {
                    index = index.saturating_sub(15);
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageDown => {
                    index = (index + 15).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Home => {
                    self.modal = Some(Modal::RepositorySearch {
                        query,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::End => {
                    index = results.len().saturating_sub(1);
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
                KeyCode::Up | KeyCode::BackTab => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Tab => {
                    index = (index + 1).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageUp => {
                    index = index.saturating_sub(15);
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageDown => {
                    index = (index + 15).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Home => {
                    self.modal = Some(Modal::FileSearch {
                        query,
                        all_files,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::End => {
                    index = results.len().saturating_sub(1);
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
                            line: None,
                            definition: false,
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
                KeyCode::Up | KeyCode::BackTab => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Tab => {
                    index = (index + 1).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageUp => {
                    index = index.saturating_sub(15);
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageDown => {
                    index = (index + 15).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Home => {
                    self.modal = Some(Modal::CodeSearch {
                        mode,
                        query,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::End => {
                    index = results.len().saturating_sub(1);
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
                            line: result.line,
                            definition: mode == CodeSearchMode::Definition,
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
                KeyCode::Up | KeyCode::BackTab => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Tab => {
                    index = (index + 1).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageUp => {
                    index = index.saturating_sub(15);
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageDown => {
                    index = (index + 15).min(results.len().saturating_sub(1));
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Home => {
                    self.modal = Some(Modal::SymbolPicker {
                        query,
                        all_symbols,
                        results,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::End => {
                    index = results.len().saturating_sub(1);
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
                    if let Some(symbol) = results.get(index)
                        && let Some(file) = self.file.as_mut()
                    {
                        file.cursor_line = symbol.line.saturating_sub(1);
                        file.viewport_top = file.cursor_line.saturating_sub(4);
                        file.tab = FileTab::Code;
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
            Modal::FindInFile {
                mut query,
                mut matches,
                mut index,
            } => match key.code {
                KeyCode::Esc => AppCommand::None,
                KeyCode::Up | KeyCode::BackTab => {
                    index = index.saturating_sub(1);
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Down | KeyCode::Tab => {
                    index = (index + 1).min(matches.len().saturating_sub(1));
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageUp => {
                    index = index.saturating_sub(15);
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::PageDown => {
                    index = (index + 15).min(matches.len().saturating_sub(1));
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Home => {
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::End => {
                    index = matches.len().saturating_sub(1);
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
                        index,
                    });
                    AppCommand::None
                }
                KeyCode::Backspace => {
                    query.pop();
                    matches = self
                        .file
                        .as_ref()
                        .map(|file| find_lines(&file.content, &query))
                        .unwrap_or_default();
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
                        index: 0,
                    });
                    AppCommand::None
                }
                KeyCode::Enter => {
                    if query.trim().is_empty() {
                        self.modal = Some(Modal::FindInFile {
                            query,
                            matches,
                            index,
                        });
                    } else {
                        self.apply_find_in_file(query, matches, index);
                    }
                    AppCommand::None
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    query.push(character);
                    matches = self
                        .file
                        .as_ref()
                        .map(|file| find_lines(&file.content, &query))
                        .unwrap_or_default();
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
                        index: 0,
                    });
                    AppCommand::None
                }
                _ => {
                    self.modal = Some(Modal::FindInFile {
                        query,
                        matches,
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
        if self.home.focus == HomeFocus::History
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('d')
        {
            if !self.home.history.is_empty() {
                self.modal = Some(Modal::ConfirmClearHistory);
            }
            return AppCommand::None;
        }

        if self.home.focus == HomeFocus::History && key.code == KeyCode::Char('d') {
            return self
                .home
                .history
                .get(self.home.history_index)
                .map_or(AppCommand::None, |entry| {
                    AppCommand::DeleteHistory(entry.repository.id.clone())
                });
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
                if self.home_at_first_item() {
                    self.home.focus = self.home.focus.previous();
                } else {
                    self.move_home_selection(-1);
                }
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.home_at_last_item() {
                    self.home.focus = self.home.focus.next();
                } else {
                    self.move_home_selection(1);
                }
                AppCommand::None
            }
            KeyCode::PageUp => {
                self.move_home_selection(-10);
                AppCommand::None
            }
            KeyCode::PageDown => {
                self.move_home_selection(10);
                AppCommand::None
            }
            KeyCode::Home => {
                self.set_home_selection(false);
                AppCommand::None
            }
            KeyCode::End => {
                self.set_home_selection(true);
                AppCommand::None
            }
            KeyCode::Enter => self.open_selected_home_item(),
            KeyCode::Char('r') | KeyCode::F(5) => {
                AppCommand::ForceRefresh(Box::new(AppCommand::RefreshHome))
            }
            KeyCode::Esc => {
                self.home.focus = HomeFocus::Search;
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_home_search_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            return AppCommand::PasteClipboard;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            return AppCommand::ForceRefresh(Box::new(AppCommand::RefreshHome));
        }
        match key.code {
            KeyCode::Enter => {
                let query = self.home.query.trim().to_owned();
                if query.is_empty() {
                    return AppCommand::None;
                }
                if let Some(id) = RepositoryId::from_exact_short(&query) {
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
            KeyCode::BackTab | KeyCode::Up => {
                self.home.focus = HomeFocus::Recommended;
                AppCommand::None
            }
            KeyCode::Esc if self.home.query.is_empty() => AppCommand::Quit,
            KeyCode::Esc => {
                self.home.query.clear();
                AppCommand::None
            }
            KeyCode::F(5) => AppCommand::ForceRefresh(Box::new(AppCommand::RefreshHome)),
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

        if repository_tab_has_state_filter(repository.tab)
            && let Some(filter) = direct_repository_state_filter(&key)
        {
            return set_repository_state_filter(repository, filter);
        }

        match key.code {
            KeyCode::Esc => {
                if repository.tab != RepositoryTab::Code {
                    self.select_repository_tab(RepositoryTab::Code)
                } else if repository.path.is_empty() {
                    self.screen = Screen::Home;
                    AppCommand::None
                } else {
                    AppCommand::OpenDirectory(repository.parent_path())
                }
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
            KeyCode::Char('[') if repository_tab_has_state_filter(repository.tab) => {
                cycle_repository_state_filter(repository, false)
            }
            KeyCode::Char(']') if repository_tab_has_state_filter(repository.tab) => {
                cycle_repository_state_filter(repository, true)
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
            KeyCode::PageDown => {
                if repository.tab == RepositoryTab::Commits
                    && repository.commit_index + 15 >= repository.commits.len()
                    && !repository.commits.is_empty()
                {
                    AppCommand::LoadCommits {
                        page: repository.commit_page.saturating_add(1),
                    }
                } else {
                    move_repository_selection(repository, 15);
                    AppCommand::None
                }
            }
            KeyCode::PageUp => {
                if repository.tab == RepositoryTab::Commits
                    && repository.commit_index < 15
                    && repository.commit_page > 1
                {
                    AppCommand::LoadCommits {
                        page: repository.commit_page.saturating_sub(1),
                    }
                } else {
                    move_repository_selection(repository, -15);
                    AppCommand::None
                }
            }
            KeyCode::Char('o') => {
                selected_external_url(repository).map_or(AppCommand::None, AppCommand::OpenExternal)
            }
            KeyCode::Char('r') | KeyCode::F(5) => {
                AppCommand::ForceRefresh(Box::new(self.reload_repository_tab()))
            }
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
            RepositoryTab::PullRequests => repository.pull_requests_loaded,
            RepositoryTab::Issues => repository.issues_loaded,
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

    fn go_to_definition(&mut self) -> AppCommand {
        let Some(file) = self.file.as_ref() else {
            return AppCommand::None;
        };
        let seed = symbols::identifier_near_cursor(file.cursor_line_text(), None);
        if seed.is_empty() {
            self.set_status("Definition: enter a symbol name");
            self.modal = Some(Modal::CodeSearch {
                mode: CodeSearchMode::Definition,
                query: String::new(),
                results: Vec::new(),
                index: 0,
            });
            return AppCommand::None;
        }
        let local_definition = symbols::find_definition(&file.path, &file.content, &seed);
        let path = file.path.clone();

        if let Some(symbol) = local_definition {
            if let Some(file) = self.file.as_mut() {
                file.cursor_line = symbol.line.saturating_sub(1);
                file.viewport_top = file.cursor_line.saturating_sub(4);
                file.tab = FileTab::Code;
            }
            self.set_status(format!(
                "Definition: {} · {path}:{}",
                symbol.kind, symbol.line
            ));
            AppCommand::None
        } else {
            self.set_status(format!("Definition: searching the repository for {seed}"));
            self.modal = Some(Modal::CodeSearch {
                mode: CodeSearchMode::Definition,
                query: seed.clone(),
                results: Vec::new(),
                index: 0,
            });
            AppCommand::SearchCode {
                query: seed,
                mode: CodeSearchMode::Definition,
            }
        }
    }

    fn handle_file_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.code == KeyCode::Char('q') {
            return AppCommand::Quit;
        }
        if key.code == KeyCode::Char('d')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return self.go_to_definition();
        }
        if (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f'))
            || (key.code == KeyCode::Char('/')
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT))
        {
            self.open_find_in_file();
            return AppCommand::None;
        }
        if key.code == KeyCode::Char('n')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT)
        {
            self.repeat_find_in_file(false);
            return AppCommand::None;
        }
        if key.code == KeyCode::Char('N')
            || (key.code == KeyCode::Char('n') && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            self.repeat_find_in_file(true);
            return AppCommand::None;
        }
        if matches!(key.code, KeyCode::F(5) | KeyCode::Char('r'))
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT)
        {
            return self.file.as_ref().map_or(AppCommand::None, |file| {
                AppCommand::ForceRefresh(Box::new(AppCommand::OpenFile {
                    path: file.path.clone(),
                    find: file.last_find.clone(),
                    line: Some(file.cursor_line.saturating_add(1)),
                    definition: false,
                }))
            });
        }
        let Some(file) = self.file.as_mut() else {
            self.screen = Screen::Repository;
            return AppCommand::None;
        };

        if file.tab != FileTab::History
            && let Some(delta) = selection_delta(key)
        {
            if file.selection_anchor.is_none() {
                file.selection_anchor = Some(file.cursor_line);
            }
            if delta < 0 {
                file.cursor_line = file.cursor_line.saturating_sub(1);
            } else {
                file.cursor_line = (file.cursor_line + 1).min(file.line_count().saturating_sub(1));
            }
            keep_cursor_visible(file);
            return AppCommand::None;
        }

        match key.code {
            KeyCode::Esc if file.selection_anchor.is_some() && file.tab != FileTab::History => {
                file.selection_anchor = None;
                AppCommand::None
            }
            KeyCode::Esc => {
                let parent = file
                    .path
                    .rsplit_once('/')
                    .map_or_else(String::new, |(parent, _)| parent.to_owned());
                AppCommand::OpenDirectory(parent)
            }
            KeyCode::Backspace | KeyCode::Char('b') => {
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
                let language = detect_language(&file.path, &file.content);
                let all_symbols = symbols::extract_symbols(&file.path, &file.content);
                let results = all_symbols.clone();
                if all_symbols.is_empty() {
                    self.set_status(format!(
                        "Symbols: no outline items detected for {} in this file",
                        language.label()
                    ));
                } else {
                    self.set_status(format!(
                        "Symbols: {} outline items detected for {}",
                        all_symbols.len(),
                        language.label()
                    ));
                }
                self.modal = Some(Modal::SymbolPicker {
                    query: String::new(),
                    all_symbols,
                    results,
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
        let line_count = commit.line_count();

        if let Some(delta) = selection_delta(key) {
            if commit.selection_anchor.is_none() {
                commit.selection_anchor = Some(commit.cursor_line);
            }
            move_reader_cursor(
                &mut commit.cursor_line,
                &mut commit.viewport_top,
                line_count,
                delta,
            );
            return AppCommand::None;
        }

        match key.code {
            KeyCode::Esc if commit.selection_anchor.is_some() => {
                commit.selection_anchor = None;
                AppCommand::None
            }
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') => {
                self.screen = Screen::Repository;
                AppCommand::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                move_reader_cursor(
                    &mut commit.cursor_line,
                    &mut commit.viewport_top,
                    line_count,
                    -1,
                );
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                move_reader_cursor(
                    &mut commit.cursor_line,
                    &mut commit.viewport_top,
                    line_count,
                    1,
                );
                AppCommand::None
            }
            KeyCode::PageUp => {
                move_reader_cursor(
                    &mut commit.cursor_line,
                    &mut commit.viewport_top,
                    line_count,
                    -20,
                );
                AppCommand::None
            }
            KeyCode::PageDown => {
                move_reader_cursor(
                    &mut commit.cursor_line,
                    &mut commit.viewport_top,
                    line_count,
                    20,
                );
                AppCommand::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                commit.cursor_line = 0;
                commit.viewport_top = 0;
                AppCommand::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                commit.cursor_line = line_count.saturating_sub(1);
                keep_reader_cursor_visible(commit.cursor_line, &mut commit.viewport_top);
                AppCommand::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                commit.horizontal_scroll = commit.horizontal_scroll.saturating_sub(4);
                AppCommand::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                commit.horizontal_scroll = commit.horizontal_scroll.saturating_add(4);
                AppCommand::None
            }
            KeyCode::Char('v') => {
                commit.selection_anchor = if commit.selection_anchor.is_some() {
                    None
                } else {
                    Some(commit.cursor_line)
                };
                AppCommand::None
            }
            KeyCode::Char('y') => AppCommand::CopyText(commit.selected_text()),
            KeyCode::Char('o') => AppCommand::OpenExternal(commit.detail.summary.html_url.clone()),
            KeyCode::Char('p') => AppCommand::ExportCommit,
            _ => AppCommand::None,
        }
    }

    fn handle_detail_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.code == KeyCode::Char('q') {
            return AppCommand::Quit;
        }
        let Some(detail) = self.detail.as_mut() else {
            self.screen = Screen::Repository;
            return AppCommand::None;
        };
        let line_count = detail.line_count();

        if let Some(delta) = selection_delta(key) {
            if detail.selection_anchor.is_none() {
                detail.selection_anchor = Some(detail.cursor_line);
            }
            move_reader_cursor(
                &mut detail.cursor_line,
                &mut detail.viewport_top,
                line_count,
                delta,
            );
            return AppCommand::None;
        }

        match key.code {
            KeyCode::Esc if detail.selection_anchor.is_some() => {
                detail.selection_anchor = None;
                AppCommand::None
            }
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') => {
                self.screen = Screen::Repository;
                AppCommand::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                move_reader_cursor(
                    &mut detail.cursor_line,
                    &mut detail.viewport_top,
                    line_count,
                    -1,
                );
                AppCommand::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                move_reader_cursor(
                    &mut detail.cursor_line,
                    &mut detail.viewport_top,
                    line_count,
                    1,
                );
                AppCommand::None
            }
            KeyCode::PageUp => {
                move_reader_cursor(
                    &mut detail.cursor_line,
                    &mut detail.viewport_top,
                    line_count,
                    -20,
                );
                AppCommand::None
            }
            KeyCode::PageDown => {
                move_reader_cursor(
                    &mut detail.cursor_line,
                    &mut detail.viewport_top,
                    line_count,
                    20,
                );
                AppCommand::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                detail.cursor_line = 0;
                detail.viewport_top = 0;
                AppCommand::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                detail.cursor_line = line_count.saturating_sub(1);
                keep_reader_cursor_visible(detail.cursor_line, &mut detail.viewport_top);
                AppCommand::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                detail.horizontal_scroll = detail.horizontal_scroll.saturating_sub(4);
                AppCommand::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                detail.horizontal_scroll = detail.horizontal_scroll.saturating_add(4);
                AppCommand::None
            }
            KeyCode::Char('v') => {
                detail.selection_anchor = if detail.selection_anchor.is_some() {
                    None
                } else {
                    Some(detail.cursor_line)
                };
                AppCommand::None
            }
            KeyCode::Char('y') => AppCommand::CopyText(detail.selected_text()),
            KeyCode::Char('o') => AppCommand::OpenExternal(detail.document.html_url().to_owned()),
            _ => AppCommand::None,
        }
    }

    fn home_at_first_item(&self) -> bool {
        match self.home.focus {
            HomeFocus::Search => true,
            HomeFocus::History => self.home.history_index == 0,
            HomeFocus::Featured => self.home.featured_index == 0,
            HomeFocus::Recommended => self.home.recommended_index == 0,
        }
    }

    fn home_at_last_item(&self) -> bool {
        match self.home.focus {
            HomeFocus::Search => true,
            HomeFocus::History => {
                self.home.history_index >= self.home.history.len().saturating_sub(1)
            }
            HomeFocus::Featured => {
                self.home.featured_index >= self.home.featured.len().saturating_sub(1)
            }
            HomeFocus::Recommended => {
                self.home.recommended_index >= self.home.recommended.len().saturating_sub(1)
            }
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

    fn set_home_selection(&mut self, end: bool) {
        let (index, len) = match self.home.focus {
            HomeFocus::Search => return,
            HomeFocus::History => (&mut self.home.history_index, self.home.history.len()),
            HomeFocus::Featured => (&mut self.home.featured_index, self.home.featured.len()),
            HomeFocus::Recommended => (
                &mut self.home.recommended_index,
                self.home.recommended.len(),
            ),
        };
        *index = if end { len.saturating_sub(1) } else { 0 };
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

fn selection_delta(key: KeyEvent) -> Option<isize> {
    if !key.modifiers.contains(KeyModifiers::SHIFT) {
        return None;
    }

    match key.code {
        KeyCode::Char('K') | KeyCode::Char('k') => Some(-1),
        KeyCode::Char('J') | KeyCode::Char('j') => Some(1),
        _ => None,
    }
}

fn repository_tab_has_state_filter(tab: RepositoryTab) -> bool {
    matches!(tab, RepositoryTab::PullRequests | RepositoryTab::Issues)
}

fn direct_repository_state_filter(key: &KeyEvent) -> Option<OpenClosedFilter> {
    let KeyCode::Char(character) = key.code else {
        return None;
    };
    match character {
        'O' => Some(OpenClosedFilter::Open),
        'C' => Some(OpenClosedFilter::Closed),
        'A' => Some(OpenClosedFilter::All),
        'o' if key.modifiers.contains(KeyModifiers::SHIFT) => Some(OpenClosedFilter::Open),
        'c' if key.modifiers.contains(KeyModifiers::SHIFT) => Some(OpenClosedFilter::Closed),
        'a' if key.modifiers.contains(KeyModifiers::SHIFT) => Some(OpenClosedFilter::All),
        _ => None,
    }
}

fn active_repository_state_filter(repository: &RepositoryState) -> Option<OpenClosedFilter> {
    match repository.tab {
        RepositoryTab::PullRequests => Some(repository.pull_request_filter),
        RepositoryTab::Issues => Some(repository.issue_filter),
        RepositoryTab::Code
        | RepositoryTab::Commits
        | RepositoryTab::Actions
        | RepositoryTab::Releases => None,
    }
}

fn cycle_repository_state_filter(repository: &mut RepositoryState, forward: bool) -> AppCommand {
    let Some(current) = active_repository_state_filter(repository) else {
        return AppCommand::None;
    };
    let next = if forward {
        current.next()
    } else {
        current.previous()
    };
    set_repository_state_filter(repository, next)
}

fn set_repository_state_filter(
    repository: &mut RepositoryState,
    filter: OpenClosedFilter,
) -> AppCommand {
    let tab = repository.tab;
    match tab {
        RepositoryTab::PullRequests => {
            if repository.pull_request_filter == filter && repository.pull_requests_loaded {
                return AppCommand::None;
            }
            repository.pull_request_filter = filter;
            repository.pull_requests.clear();
            repository.pull_requests_loaded = false;
        }
        RepositoryTab::Issues => {
            if repository.issue_filter == filter && repository.issues_loaded {
                return AppCommand::None;
            }
            repository.issue_filter = filter;
            repository.issues.clear();
            repository.issues_loaded = false;
        }
        RepositoryTab::Code
        | RepositoryTab::Commits
        | RepositoryTab::Actions
        | RepositoryTab::Releases => return AppCommand::None,
    }
    repository.list_index = 0;
    AppCommand::LoadRepositoryTab(tab)
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
                        line: None,
                        definition: false,
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
                AppCommand::OpenPullRequest(item.number)
            }),
        RepositoryTab::Issues => repository
            .issues
            .get(repository.list_index)
            .map_or(AppCommand::None, |item| AppCommand::OpenIssue(item.number)),
        RepositoryTab::Actions => repository
            .workflow_runs
            .get(repository.list_index)
            .map_or(AppCommand::None, |item| {
                AppCommand::OpenWorkflowRun(item.id)
            }),
        RepositoryTab::Releases => repository
            .releases
            .get(repository.list_index)
            .map_or(AppCommand::None, |item| AppCommand::OpenRelease(item.id)),
    }
}

fn selected_external_url(repository: &RepositoryState) -> Option<String> {
    match repository.tab {
        RepositoryTab::Code => Some(repository.repository.html_url.clone()),
        RepositoryTab::Commits => repository
            .selected_commit()
            .map(|commit| commit.html_url.clone()),
        RepositoryTab::PullRequests => repository
            .pull_requests
            .get(repository.list_index)
            .map(|item| item.html_url.clone())
            .or_else(|| Some(format!("{}/pulls", repository.repository.html_url))),
        RepositoryTab::Issues => repository
            .issues
            .get(repository.list_index)
            .map(|item| item.html_url.clone())
            .or_else(|| Some(format!("{}/issues", repository.repository.html_url))),
        RepositoryTab::Actions => repository
            .workflow_runs
            .get(repository.list_index)
            .map(|item| item.html_url.clone()),
        RepositoryTab::Releases => repository
            .releases
            .get(repository.list_index)
            .map(|item| item.html_url.clone()),
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
        *index = (*index).saturating_sub(delta.unsigned_abs());
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

fn rank_repository_results(query: &str, results: &mut [RepoCard]) {
    let normalized = query.trim().to_ascii_lowercase();
    results.sort_by_key(|card| {
        let full_name = card.id.full_name().to_ascii_lowercase();
        let name = card.id.name.to_ascii_lowercase();
        let description = card
            .description
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let match_rank = if full_name == normalized {
            0
        } else if name == normalized {
            1
        } else if full_name.starts_with(&normalized) {
            2
        } else if name.starts_with(&normalized) {
            3
        } else if full_name.contains(&normalized) {
            4
        } else if description.contains(&normalized) {
            5
        } else {
            6
        };
        (match_rank, std::cmp::Reverse(card.stars), full_name)
    });
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

fn find_lines(content: &str, query: &str) -> Vec<usize> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    content
        .lines()
        .enumerate()
        .filter_map(|(line, value)| value.to_ascii_lowercase().contains(&query).then_some(line))
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

fn with_parent_entry(path: &str, mut entries: Vec<ContentEntry>) -> Vec<ContentEntry> {
    if !path.is_empty() {
        let parent = path
            .rsplit_once('/')
            .map_or_else(String::new, |(parent, _)| parent.to_owned());
        entries.insert(
            0,
            ContentEntry {
                name: "..".to_owned(),
                path: parent,
                sha: String::new(),
                size: 0,
                kind: ContentKind::Directory,
            },
        );
    }
    entries
}

fn selected_lines(text: &str, anchor: Option<usize>, cursor: usize) -> String {
    let anchor = anchor.unwrap_or(cursor);
    let start = anchor.min(cursor);
    let end = anchor.max(cursor);
    text.lines()
        .skip(start)
        .take(end.saturating_sub(start) + 1)
        .collect::<Vec<_>>()
        .join("\n")
}

fn keep_reader_cursor_visible(cursor: usize, viewport_top: &mut usize) {
    const VIEWPORT_HINT: usize = 24;
    if cursor < *viewport_top {
        *viewport_top = cursor;
    } else if cursor >= *viewport_top + VIEWPORT_HINT {
        *viewport_top = cursor.saturating_sub(VIEWPORT_HINT - 1);
    }
}

fn move_reader_cursor(
    cursor: &mut usize,
    viewport_top: &mut usize,
    line_count: usize,
    delta: isize,
) {
    if delta.is_negative() {
        *cursor = (*cursor).saturating_sub(delta.unsigned_abs());
    } else {
        *cursor = (*cursor + delta as usize).min(line_count.saturating_sub(1));
    }
    keep_reader_cursor_visible(*cursor, viewport_top);
}

fn push_comments(output: &mut String, comments: &[Comment]) {
    if comments.is_empty() {
        return;
    }
    output.push_str("\nComments\n========\n");
    for comment in comments {
        output.push_str(&format!(
            "\n{} · {}\n{}\n",
            comment.author,
            comment.created_at.format("%Y-%m-%d %H:%M UTC"),
            comment.body.trim()
        ));
    }
}

fn push_numbered_patch(output: &mut String, patch: &str) {
    for line in parse_patch(patch) {
        match line.kind {
            DiffKind::Hunk | DiffKind::Meta => {
                output.push_str(&line.text);
                output.push('\n');
            }
            DiffKind::Add | DiffKind::Delete | DiffKind::Context => {
                let old = line
                    .old_line
                    .map_or_else(|| "     ".to_owned(), |value| format!("{value:>5}"));
                let new = line
                    .new_line
                    .map_or_else(|| "     ".to_owned(), |value| format!("{value:>5}"));
                let sign = match line.kind {
                    DiffKind::Add => '+',
                    DiffKind::Delete => '-',
                    DiffKind::Context => ' ',
                    DiffKind::Hunk | DiffKind::Meta => ' ',
                };
                output.push_str(&format!("{old} {new} {sign} {}\n", line.text));
            }
        }
    }
}

fn commit_text(detail: &CommitDetail) -> String {
    let mut output = format!(
        "{}\n\nAuthor: {}\nCommit: {}\nFiles: {}  +{}  -{}\n",
        detail.summary.title,
        detail.summary.author_name,
        detail.summary.sha,
        detail.files.len(),
        detail.stats.additions,
        detail.stats.deletions
    );
    if !detail.summary.body.trim().is_empty() {
        output.push('\n');
        output.push_str(detail.summary.body.trim());
        output.push('\n');
    }
    for file in &detail.files {
        output.push_str(&format!(
            "\n--- {} · {} · +{} -{}\n",
            file.filename, file.status, file.additions, file.deletions
        ));
        if let Some(patch) = &file.patch {
            push_numbered_patch(&mut output, patch);
        } else {
            output.push_str("(patch unavailable)\n");
        }
    }
    output
}

fn detail_text(document: &DetailDocument) -> String {
    match document {
        DetailDocument::PullRequest(value) => {
            let mut output = format!(
                "#{} {}\n\nState: {}{}\nAuthor: {}\nBranch: {} → {}\nCommits: {} · Files: {} · +{} -{}\n",
                value.summary.number,
                value.summary.title,
                value.state,
                if value.merged { " · merged" } else { "" },
                value.summary.author,
                value.summary.head,
                value.summary.base,
                value.commits,
                value.changed_files,
                value.additions,
                value.deletions
            );
            if !value.body.trim().is_empty() {
                output.push('\n');
                output.push_str(value.body.trim());
                output.push('\n');
            }
            for file in &value.files {
                output.push_str(&format!(
                    "\n--- {} · {} · +{} -{}\n",
                    file.filename, file.status, file.additions, file.deletions
                ));
                if let Some(patch) = &file.patch {
                    push_numbered_patch(&mut output, patch);
                }
            }
            push_comments(&mut output, &value.comments);
            output
        }
        DetailDocument::Issue(value) => {
            let mut output = format!(
                "#{} {}\n\nState: {}\nAuthor: {}\nLabels: {}\n",
                value.summary.number,
                value.summary.title,
                value.state,
                value.summary.author,
                if value.summary.labels.is_empty() {
                    "-".to_owned()
                } else {
                    value.summary.labels.join(", ")
                }
            );
            if !value.body.trim().is_empty() {
                output.push('\n');
                output.push_str(value.body.trim());
                output.push('\n');
            }
            push_comments(&mut output, &value.comments);
            output
        }
        DetailDocument::WorkflowRun(value) => {
            let mut output = format!(
                "{}\n\nStatus: {} / {}\nBranch: {}\nEvent: {}\n",
                value.summary.name,
                value.summary.status,
                value.summary.conclusion.as_deref().unwrap_or("pending"),
                value.summary.branch,
                value.summary.event
            );
            for job in &value.jobs {
                output.push_str(&format!(
                    "\nJob: {} · {} / {}\n",
                    job.name,
                    job.status,
                    job.conclusion.as_deref().unwrap_or("pending")
                ));
                for step in &job.steps {
                    output.push_str(&format!(
                        "  {:>2}. {} · {} / {}\n",
                        step.number,
                        step.name,
                        step.status,
                        step.conclusion.as_deref().unwrap_or("pending")
                    ));
                }
            }
            output
        }
        DetailDocument::Release(value) => {
            let mut output = format!(
                "{}\n\nAuthor: {}\nTag: {}{}{}\n",
                value
                    .summary
                    .name
                    .as_deref()
                    .unwrap_or(&value.summary.tag_name),
                value.author,
                value.summary.tag_name,
                if value.summary.draft { " · draft" } else { "" },
                if value.summary.prerelease {
                    " · prerelease"
                } else {
                    ""
                }
            );
            if !value.body.trim().is_empty() {
                output.push('\n');
                output.push_str(value.body.trim());
                output.push('\n');
            }
            if !value.assets.is_empty() {
                output.push_str("\nAssets\n======\n");
                for asset in &value.assets {
                    output.push_str(&format!(
                        "{} · {} bytes · {} downloads\n",
                        asset.name, asset.size, asset.download_count
                    ));
                }
            }
            output
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
