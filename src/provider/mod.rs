pub mod github;

use thiserror::Error;

use crate::model::{
    ApiResponse, BlameRange, BranchSummary, CodeSearchResult, CommitDetail, CommitSummary,
    ContentEntry, IssueDetail, IssueSummary, PullRequestDetail, PullRequestSummary, RateLimit,
    ReleaseDetail, ReleaseSummary, RepoCard, Repository, RepositoryId, TreeEntry,
    WorkflowRunDetail, WorkflowRunSummary,
};

pub type ProviderResult<T> = Result<ApiResponse<T>, ProviderError>;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Failed to communicate with GitHub: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("GitHub API returned HTTP {status}: {message}")]
    Api {
        status: u16,
        message: String,
        rate_limit: RateLimit,
    },

    #[error("Anonymous GitHub API quota has been exhausted")]
    RateLimited(RateLimit),

    #[error("This operation requires GitHub authentication")]
    AuthenticationRequired { rate_limit: RateLimit },

    #[error("GitHub API temporarily limited the request: {message}")]
    TemporarilyLimited {
        message: String,
        retry_after_seconds: Option<u64>,
        rate_limit: RateLimit,
    },

    #[error("Could not interpret the GitHub API response: {0}")]
    InvalidResponse(String),

    #[error("The file is {size} bytes, larger than the display limit of {limit} bytes")]
    FileTooLarge { size: usize, limit: usize },

    #[error("The selected file is not UTF-8 text")]
    BinaryFile,
}

impl ProviderError {
    #[must_use]
    pub fn rate_limit(&self) -> Option<&RateLimit> {
        match self {
            Self::Api { rate_limit, .. }
            | Self::TemporarilyLimited { rate_limit, .. }
            | Self::RateLimited(rate_limit)
            | Self::AuthenticationRequired { rate_limit } => Some(rate_limit),
            Self::Transport(_)
            | Self::InvalidResponse(_)
            | Self::FileTooLarge { .. }
            | Self::BinaryFile => None,
        }
    }
}

pub trait RepositoryProvider {
    fn repository(&self, id: &RepositoryId) -> ProviderResult<Repository>;

    fn contents(
        &self,
        id: &RepositoryId,
        path: &str,
        git_ref: &str,
    ) -> ProviderResult<Vec<ContentEntry>>;

    fn file_content(&self, id: &RepositoryId, path: &str, git_ref: &str) -> ProviderResult<String>;

    fn branches(&self, id: &RepositoryId) -> ProviderResult<Vec<BranchSummary>>;

    fn tree(&self, id: &RepositoryId, git_ref: &str) -> ProviderResult<Vec<TreeEntry>>;

    fn commits(
        &self,
        id: &RepositoryId,
        git_ref: &str,
        page: u32,
        per_page: u32,
    ) -> ProviderResult<Vec<CommitSummary>>;

    fn file_history(
        &self,
        id: &RepositoryId,
        git_ref: &str,
        path: &str,
        page: u32,
        per_page: u32,
    ) -> ProviderResult<Vec<CommitSummary>>;

    fn commit(&self, id: &RepositoryId, sha: &str) -> ProviderResult<CommitDetail>;

    fn blame(
        &self,
        id: &RepositoryId,
        git_ref: &str,
        path: &str,
    ) -> ProviderResult<Vec<BlameRange>>;

    fn search_repositories(
        &self,
        query: &str,
        sort: &str,
        per_page: u32,
    ) -> ProviderResult<Vec<RepoCard>>;

    fn search_code(
        &self,
        id: &RepositoryId,
        query: &str,
        per_page: u32,
    ) -> ProviderResult<Vec<CodeSearchResult>>;

    fn pull_requests(&self, id: &RepositoryId) -> ProviderResult<Vec<PullRequestSummary>>;

    fn pull_request(&self, id: &RepositoryId, number: u64) -> ProviderResult<PullRequestDetail>;

    fn issues(&self, id: &RepositoryId) -> ProviderResult<Vec<IssueSummary>>;

    fn issue(&self, id: &RepositoryId, number: u64) -> ProviderResult<IssueDetail>;

    fn workflow_runs(
        &self,
        id: &RepositoryId,
        git_ref: &str,
    ) -> ProviderResult<Vec<WorkflowRunSummary>>;

    fn workflow_run(&self, id: &RepositoryId, run_id: u64) -> ProviderResult<WorkflowRunDetail>;

    fn releases(&self, id: &RepositoryId) -> ProviderResult<Vec<ReleaseSummary>>;

    fn release(&self, id: &RepositoryId, release_id: u64) -> ProviderResult<ReleaseDetail>;

    fn viewer_login(&self) -> ProviderResult<String>;
}
