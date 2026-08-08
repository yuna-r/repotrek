pub mod github;

use thiserror::Error;

use crate::model::{
    ApiResponse, CommitDetail, CommitSummary, ContentEntry, RateLimit, RepoCard, Repository,
    RepositoryId,
};

pub type ProviderResult<T> = Result<ApiResponse<T>, ProviderError>;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("GitHubへの通信に失敗しました: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("GitHub APIがHTTP {status}を返しました: {message}")]
    Api {
        status: u16,
        message: String,
        rate_limit: RateLimit,
    },

    #[error("匿名APIリクエスト上限に達しました")]
    RateLimited(RateLimit),

    #[error("GitHub APIが一時的なアクセス制限を返しました: {message}")]
    TemporarilyLimited {
        message: String,
        retry_after_seconds: Option<u64>,
        rate_limit: RateLimit,
    },

    #[error("GitHub APIの応答を解釈できませんでした: {0}")]
    InvalidResponse(String),

    #[error("{size}バイトのファイルはMVPの表示上限{limit}バイトを超えています")]
    FileTooLarge { size: usize, limit: usize },

    #[error("このファイルはUTF-8テキストではありません")]
    BinaryFile,
}

impl ProviderError {
    #[must_use]
    pub fn rate_limit(&self) -> Option<&RateLimit> {
        match self {
            Self::Api { rate_limit, .. }
            | Self::TemporarilyLimited { rate_limit, .. }
            | Self::RateLimited(rate_limit) => Some(rate_limit),
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

    fn commits(
        &self,
        id: &RepositoryId,
        git_ref: &str,
        page: u32,
        per_page: u32,
    ) -> ProviderResult<Vec<CommitSummary>>;

    fn commit(&self, id: &RepositoryId, sha: &str) -> ProviderResult<CommitDetail>;

    fn search_repositories(
        &self,
        query: &str,
        sort: &str,
        per_page: u32,
    ) -> ProviderResult<Vec<RepoCard>>;
}
