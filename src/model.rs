use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositoryId {
    pub owner: String,
    pub name: String,
}

impl RepositoryId {
    #[must_use]
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

impl fmt::Display for RepositoryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RepositoryIdParseError {
    #[error("Enter owner/repo or a GitHub repository URL")]
    InvalidFormat,
    #[error("The GitHub provider currently supports github.com URLs")]
    UnsupportedHost,
}

impl FromStr for RepositoryId {
    type Err = RepositoryIdParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let value = input.trim().trim_end_matches('/');
        if value.is_empty() {
            return Err(RepositoryIdParseError::InvalidFormat);
        }

        if let Some(rest) = value.strip_prefix("git@github.com:") {
            return parse_owner_repo(rest);
        }
        if let Some(rest) = value.strip_prefix("github.com/") {
            return parse_owner_repo(rest);
        }
        if value.contains("://") {
            let url = Url::parse(value).map_err(|_| RepositoryIdParseError::InvalidFormat)?;
            let host = url
                .host_str()
                .ok_or(RepositoryIdParseError::InvalidFormat)?;
            if host != "github.com" && host != "www.github.com" {
                return Err(RepositoryIdParseError::UnsupportedHost);
            }
            return parse_owner_repo(url.path().trim_start_matches('/'));
        }

        parse_owner_repo(value)
    }
}

fn parse_owner_repo(value: &str) -> Result<RepositoryId, RepositoryIdParseError> {
    let mut parts = value.split('/').filter(|part| !part.is_empty());
    let owner = parts.next().ok_or(RepositoryIdParseError::InvalidFormat)?;
    let name = parts
        .next()
        .ok_or(RepositoryIdParseError::InvalidFormat)?
        .trim_end_matches(".git");

    if owner.is_empty()
        || name.is_empty()
        || owner.chars().any(char::is_whitespace)
        || name.chars().any(char::is_whitespace)
    {
        return Err(RepositoryIdParseError::InvalidFormat);
    }

    Ok(RepositoryId {
        owner: owner.to_owned(),
        name: name.to_owned(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: RepositoryId,
    pub full_name: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub html_url: String,
    pub stargazers_count: u64,
    pub forks_count: u64,
    pub language: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSummary {
    pub name: String,
    pub sha: String,
    pub protected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentKind {
    Directory,
    File,
    Symlink,
    Submodule,
    Unknown,
}

impl ContentKind {
    #[must_use]
    pub fn is_directory(self) -> bool {
        matches!(self, Self::Directory)
    }

    #[must_use]
    pub fn is_file(self) -> bool {
        matches!(self, Self::File | Self::Symlink)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentEntry {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub size: u64,
    pub kind: ContentKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    pub sha: String,
    pub kind: String,
    pub size: Option<u64>,
}

impl TreeEntry {
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.kind == "blob"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSummary {
    pub sha: String,
    pub title: String,
    pub body: String,
    pub author_name: String,
    pub authored_at: Option<DateTime<Utc>>,
    pub html_url: String,
    pub verified: bool,
    pub parent_shas: Vec<String>,
}

impl CommitSummary {
    #[must_use]
    pub fn short_sha(&self) -> &str {
        self.sha.get(..7).unwrap_or(&self.sha)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommitStats {
    pub additions: u64,
    pub deletions: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitFile {
    pub filename: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub changes: u64,
    pub patch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDetail {
    pub summary: CommitSummary,
    pub stats: CommitStats,
    pub files: Vec<CommitFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoCard {
    pub id: RepositoryId,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars: u64,
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<&Repository> for RepoCard {
    fn from(repository: &Repository) -> Self {
        Self {
            id: repository.id.clone(),
            description: repository.description.clone(),
            language: repository.language.clone(),
            stars: repository.stargazers_count,
            updated_at: Some(repository.updated_at),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head: String,
    pub base: String,
    pub draft: bool,
    pub comments: u64,
    pub updated_at: DateTime<Utc>,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueSummary {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub comments: u64,
    pub labels: Vec<String>,
    pub updated_at: DateTime<Utc>,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunSummary {
    pub id: u64,
    pub name: String,
    pub event: String,
    pub branch: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub created_at: DateTime<Utc>,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseSummary {
    pub id: u64,
    pub tag_name: String,
    pub name: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchResult {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameRange {
    pub starting_line: usize,
    pub ending_line: usize,
    pub age: u8,
    pub commit_sha: String,
    pub commit_short_sha: String,
    pub author: String,
    pub authored_at: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolLocation {
    pub name: String,
    pub kind: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub repository: RepoCard,
    pub last_path: Option<String>,
    pub last_screen: HistoryScreen,
    pub visited_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryScreen {
    #[default]
    Code,
    Commits,
    File,
    Commit,
}

#[derive(Debug, Clone, Default)]
pub struct RateLimit {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset_epoch: Option<i64>,
    pub resource: Option<String>,
}

impl RateLimit {
    #[must_use]
    pub fn exhausted(&self) -> bool {
        self.remaining == Some(0)
    }

    #[must_use]
    pub fn reset_at(&self) -> Option<DateTime<Utc>> {
        self.reset_epoch
            .and_then(|epoch| DateTime::<Utc>::from_timestamp(epoch, 0))
    }
}

#[derive(Debug, Clone)]
pub struct ApiResponse<T> {
    pub value: T,
    pub rate_limit: RateLimit,
}

#[cfg(test)]
mod tests {
    use super::RepositoryId;

    #[test]
    fn parses_short_form() {
        let parsed: RepositoryId = "rust-lang/rust".parse().expect("valid repository");
        assert_eq!(parsed.owner, "rust-lang");
        assert_eq!(parsed.name, "rust");
    }

    #[test]
    fn parses_https_url_and_ignores_deep_path() {
        let parsed: RepositoryId = "https://github.com/torvalds/linux/tree/master/kernel"
            .parse()
            .expect("valid GitHub URL");
        assert_eq!(parsed.full_name(), "torvalds/linux");
    }

    #[test]
    fn rejects_non_github_url() {
        let error = "https://example.com/owner/repo"
            .parse::<RepositoryId>()
            .expect_err("unsupported host");
        assert_eq!(error, super::RepositoryIdParseError::UnsupportedHost);
    }
}
