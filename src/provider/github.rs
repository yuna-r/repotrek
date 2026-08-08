use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{
    StatusCode, Url,
    blocking::{Client, RequestBuilder, Response},
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER},
};
use serde::Deserialize;

use crate::{
    model::{
        ApiResponse, CommitDetail, CommitFile, CommitStats, CommitSummary, ContentEntry,
        ContentKind, RateLimit, RepoCard, Repository, RepositoryId,
    },
    provider::{ProviderError, ProviderResult, RepositoryProvider},
};

const API_BASE: &str = "https://api.github.com";
const API_VERSION: &str = "2026-03-10";
const JSON_ACCEPT: &str = "application/vnd.github+json";
const RAW_ACCEPT: &str = "application/vnd.github.raw+json";
const MAX_TEXT_FILE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone)]
pub struct GitHubProvider {
    client: Client,
    token: Option<String>,
}

impl GitHubProvider {
    pub fn new(token: Option<String>) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .user_agent(concat!(
                "RepoTrek/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/yuna-r/repotrek)"
            ))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self { client, token })
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    fn request(&self, request: RequestBuilder, accept: &'static str) -> RequestBuilder {
        let mut request = request
            .header(ACCEPT, accept)
            .header("X-GitHub-Api-Version", API_VERSION);

        if let Some(token) = &self.token {
            let value = format!("Bearer {token}");
            if let Ok(mut value) = HeaderValue::from_str(&value) {
                value.set_sensitive(true);
                request = request.header(AUTHORIZATION, value);
            }
        }

        request
    }

    fn send_json<T>(&self, request: RequestBuilder) -> ProviderResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response = self.request(request, JSON_ACCEPT).send()?;
        let rate_limit = parse_rate_limit(response.headers());
        let response = ensure_success(response, rate_limit.clone())?;
        let value = response
            .json::<T>()
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        Ok(ApiResponse { value, rate_limit })
    }

    fn send_text(&self, request: RequestBuilder) -> ProviderResult<String> {
        let response = self.request(request, RAW_ACCEPT).send()?;
        let rate_limit = parse_rate_limit(response.headers());
        let response = ensure_success(response, rate_limit.clone())?;
        let bytes = response.bytes()?;

        if bytes.len() > MAX_TEXT_FILE_BYTES {
            return Err(ProviderError::FileTooLarge {
                size: bytes.len(),
                limit: MAX_TEXT_FILE_BYTES,
            });
        }

        let value = String::from_utf8(bytes.to_vec()).map_err(|_| ProviderError::BinaryFile)?;
        Ok(ApiResponse { value, rate_limit })
    }

    fn repo_url(&self, id: &RepositoryId, endpoint: &[&str]) -> Result<Url, ProviderError> {
        let mut url = Url::parse(API_BASE)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        {
            let mut segments = url.path_segments_mut().map_err(|()| {
                ProviderError::InvalidResponse("GitHub API URLを構築できません".to_owned())
            })?;
            segments.extend(["repos", id.owner.as_str(), id.name.as_str()]);
            segments.extend(endpoint.iter().copied());
        }
        Ok(url)
    }

    fn contents_url(&self, id: &RepositoryId, path: &str) -> Result<Url, ProviderError> {
        let mut endpoint = vec!["contents"];
        endpoint.extend(path.split('/').filter(|segment| !segment.is_empty()));
        self.repo_url(id, &endpoint)
    }
}

impl RepositoryProvider for GitHubProvider {
    fn repository(&self, id: &RepositoryId) -> ProviderResult<Repository> {
        let url = self.repo_url(id, &[])?;
        let response: ApiResponse<RepositoryDto> = self.send_json(self.client.get(url))?;
        Ok(ApiResponse {
            value: response.value.into_repository(),
            rate_limit: response.rate_limit,
        })
    }

    fn contents(
        &self,
        id: &RepositoryId,
        path: &str,
        git_ref: &str,
    ) -> ProviderResult<Vec<ContentEntry>> {
        let url = self.contents_url(id, path)?;
        let response: ApiResponse<Vec<ContentDto>> =
            self.send_json(self.client.get(url).query(&[("ref", git_ref)]))?;
        let mut entries: Vec<ContentEntry> = response
            .value
            .into_iter()
            .map(ContentDto::into_content)
            .collect();
        entries.sort_by(|left, right| {
            let left_rank = if left.kind.is_directory() { 0 } else { 1 };
            let right_rank = if right.kind.is_directory() { 0 } else { 1 };
            left_rank
                .cmp(&right_rank)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(ApiResponse {
            value: entries,
            rate_limit: response.rate_limit,
        })
    }

    fn file_content(&self, id: &RepositoryId, path: &str, git_ref: &str) -> ProviderResult<String> {
        let url = self.contents_url(id, path)?;
        self.send_text(self.client.get(url).query(&[("ref", git_ref)]))
    }

    fn commits(
        &self,
        id: &RepositoryId,
        git_ref: &str,
        page: u32,
        per_page: u32,
    ) -> ProviderResult<Vec<CommitSummary>> {
        let url = self.repo_url(id, &["commits"])?;
        let response: ApiResponse<Vec<CommitDto>> =
            self.send_json(self.client.get(url).query(&[
                ("sha", git_ref.to_owned()),
                ("page", page.to_string()),
                ("per_page", per_page.clamp(1, 100).to_string()),
            ]))?;
        Ok(ApiResponse {
            value: response
                .value
                .into_iter()
                .map(CommitDto::into_summary)
                .collect(),
            rate_limit: response.rate_limit,
        })
    }

    fn commit(&self, id: &RepositoryId, sha: &str) -> ProviderResult<CommitDetail> {
        let url = self.repo_url(id, &["commits", sha])?;
        let response: ApiResponse<CommitDetailDto> = self.send_json(self.client.get(url))?;
        Ok(ApiResponse {
            value: response.value.into_detail(),
            rate_limit: response.rate_limit,
        })
    }

    fn search_repositories(
        &self,
        query: &str,
        sort: &str,
        per_page: u32,
    ) -> ProviderResult<Vec<RepoCard>> {
        let url = Url::parse("https://api.github.com/search/repositories")
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let response: ApiResponse<SearchResponseDto> =
            self.send_json(self.client.get(url).query(&[
                ("q", query.to_owned()),
                ("sort", sort.to_owned()),
                ("order", "desc".to_owned()),
                ("per_page", per_page.clamp(1, 30).to_string()),
            ]))?;
        Ok(ApiResponse {
            value: response
                .value
                .items
                .into_iter()
                .map(SearchRepositoryDto::into_card)
                .collect(),
            rate_limit: response.rate_limit,
        })
    }
}

fn ensure_success(response: Response, rate_limit: RateLimit) -> Result<Response, ProviderError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let retry_after_seconds = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let body = response.text().unwrap_or_default();
    let message = serde_json::from_str::<ErrorDto>(&body)
        .map(|error| error.message)
        .unwrap_or_else(|_| {
            if body.trim().is_empty() {
                status
                    .canonical_reason()
                    .unwrap_or("GitHub API error")
                    .to_owned()
            } else {
                body
            }
        });

    if matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
    ) {
        if rate_limit.exhausted()
            && rate_limit
                .resource
                .as_deref()
                .is_none_or(|resource| resource == "core")
        {
            return Err(ProviderError::RateLimited(rate_limit));
        }
        return Err(ProviderError::TemporarilyLimited {
            message,
            retry_after_seconds,
            rate_limit,
        });
    }

    Err(ProviderError::Api {
        status: status.as_u16(),
        message,
        rate_limit,
    })
}

fn parse_rate_limit(headers: &HeaderMap) -> RateLimit {
    fn parse_u32(headers: &HeaderMap, name: &'static str) -> Option<u32> {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
    }

    fn parse_i64(headers: &HeaderMap, name: &'static str) -> Option<i64> {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
    }

    RateLimit {
        limit: parse_u32(headers, "x-ratelimit-limit"),
        remaining: parse_u32(headers, "x-ratelimit-remaining"),
        reset_epoch: parse_i64(headers, "x-ratelimit-reset"),
        resource: headers
            .get("x-ratelimit-resource")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
    }
}

#[derive(Debug, Deserialize)]
struct ErrorDto {
    message: String,
}

#[derive(Debug, Deserialize)]
struct RepositoryDto {
    full_name: String,
    description: Option<String>,
    default_branch: String,
    html_url: String,
    stargazers_count: u64,
    forks_count: u64,
    language: Option<String>,
    updated_at: DateTime<Utc>,
    private: bool,
    owner: OwnerDto,
    name: String,
}

impl RepositoryDto {
    fn into_repository(self) -> Repository {
        Repository {
            id: RepositoryId {
                owner: self.owner.login,
                name: self.name,
            },
            full_name: self.full_name,
            description: self.description,
            default_branch: self.default_branch,
            html_url: self.html_url,
            stargazers_count: self.stargazers_count,
            forks_count: self.forks_count,
            language: self.language,
            updated_at: self.updated_at,
            is_private: self.private,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OwnerDto {
    login: String,
}

#[derive(Debug, Deserialize)]
struct ContentDto {
    name: String,
    path: String,
    sha: String,
    size: u64,
    #[serde(rename = "type")]
    kind: String,
}

impl ContentDto {
    fn into_content(self) -> ContentEntry {
        let kind = match self.kind.as_str() {
            "dir" => ContentKind::Directory,
            "file" => ContentKind::File,
            "symlink" => ContentKind::Symlink,
            "submodule" => ContentKind::Submodule,
            _ => ContentKind::Unknown,
        };
        ContentEntry {
            name: self.name,
            path: self.path,
            sha: self.sha,
            size: self.size,
            kind,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CommitDto {
    sha: String,
    html_url: String,
    commit: InnerCommitDto,
    parents: Vec<ParentDto>,
    author: Option<OwnerDto>,
}

impl CommitDto {
    fn into_summary(self) -> CommitSummary {
        into_commit_summary(
            self.sha,
            self.html_url,
            self.commit,
            self.parents,
            self.author,
        )
    }
}

#[derive(Debug, Deserialize)]
struct InnerCommitDto {
    message: String,
    author: Option<SignatureDto>,
    committer: Option<SignatureDto>,
    verification: Option<VerificationDto>,
}

#[derive(Debug, Deserialize)]
struct SignatureDto {
    name: String,
    date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct VerificationDto {
    verified: bool,
}

#[derive(Debug, Deserialize)]
struct ParentDto {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct CommitDetailDto {
    sha: String,
    html_url: String,
    commit: InnerCommitDto,
    parents: Vec<ParentDto>,
    author: Option<OwnerDto>,
    stats: Option<CommitStatsDto>,
    files: Option<Vec<CommitFileDto>>,
}

impl CommitDetailDto {
    fn into_detail(self) -> CommitDetail {
        let summary = into_commit_summary(
            self.sha,
            self.html_url,
            self.commit,
            self.parents,
            self.author,
        );
        let stats = self
            .stats
            .map_or_else(CommitStats::default, |stats| CommitStats {
                additions: stats.additions,
                deletions: stats.deletions,
                total: stats.total,
            });
        let files = self
            .files
            .unwrap_or_default()
            .into_iter()
            .map(CommitFileDto::into_file)
            .collect();
        CommitDetail {
            summary,
            stats,
            files,
        }
    }
}

fn into_commit_summary(
    sha: String,
    html_url: String,
    commit: InnerCommitDto,
    parents: Vec<ParentDto>,
    github_author: Option<OwnerDto>,
) -> CommitSummary {
    let mut message_lines = commit.message.lines();
    let title = message_lines.next().unwrap_or_default().to_owned();
    let body = message_lines.collect::<Vec<_>>().join("\n");
    let author_name = commit
        .author
        .as_ref()
        .map(|author| author.name.clone())
        .or_else(|| github_author.map(|author| author.login))
        .or_else(|| commit.committer.as_ref().map(|author| author.name.clone()))
        .unwrap_or_else(|| "Unknown".to_owned());
    let authored_at = commit
        .author
        .as_ref()
        .and_then(|author| author.date)
        .or_else(|| commit.committer.as_ref().and_then(|author| author.date));
    let verified = commit
        .verification
        .is_some_and(|verification| verification.verified);

    CommitSummary {
        sha,
        title,
        body,
        author_name,
        authored_at,
        html_url,
        verified,
        parent_shas: parents.into_iter().map(|parent| parent.sha).collect(),
    }
}

#[derive(Debug, Deserialize)]
struct CommitStatsDto {
    additions: u64,
    deletions: u64,
    total: u64,
}

#[derive(Debug, Deserialize)]
struct CommitFileDto {
    filename: String,
    status: String,
    additions: u64,
    deletions: u64,
    changes: u64,
    patch: Option<String>,
}

impl CommitFileDto {
    fn into_file(self) -> CommitFile {
        CommitFile {
            filename: self.filename,
            status: self.status,
            additions: self.additions,
            deletions: self.deletions,
            changes: self.changes,
            patch: self.patch,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponseDto {
    items: Vec<SearchRepositoryDto>,
}

#[derive(Debug, Deserialize)]
struct SearchRepositoryDto {
    full_name: String,
    description: Option<String>,
    language: Option<String>,
    stargazers_count: u64,
    updated_at: Option<DateTime<Utc>>,
}

impl SearchRepositoryDto {
    fn into_card(self) -> RepoCard {
        let id = self.full_name.parse().unwrap_or_else(|_| RepositoryId {
            owner: "unknown".to_owned(),
            name: self.full_name.replace('/', "-"),
        });
        RepoCard {
            id,
            description: self.description,
            language: self.language,
            stars: self.stargazers_count,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CommitDetailDto, ContentDto, RepositoryDto};

    #[test]
    fn parses_repository_response() {
        let value = serde_json::json!({
            "full_name": "yuna-r/repotrek",
            "description": "A terminal source browser",
            "default_branch": "main",
            "html_url": "https://github.com/yuna-r/repotrek",
            "stargazers_count": 12,
            "forks_count": 3,
            "language": "Rust",
            "updated_at": "2026-08-08T00:00:00Z",
            "private": false,
            "owner": { "login": "yuna-r" },
            "name": "repotrek"
        });
        let repository: RepositoryDto =
            serde_json::from_value(value).expect("valid repository response");
        let repository = repository.into_repository();
        assert_eq!(repository.full_name, "yuna-r/repotrek");
        assert_eq!(repository.default_branch, "main");
    }

    #[test]
    fn parses_directory_entry() {
        let value = serde_json::json!({
            "name": "src",
            "path": "src",
            "sha": "0123456789abcdef",
            "size": 0,
            "type": "dir"
        });
        let entry: ContentDto = serde_json::from_value(value).expect("valid content response");
        let entry = entry.into_content();
        assert!(entry.kind.is_directory());
        assert_eq!(entry.path, "src");
    }

    #[test]
    fn parses_commit_detail_response() {
        let value = serde_json::json!({
            "sha": "0123456789abcdef",
            "html_url": "https://github.com/yuna-r/repotrek/commit/0123456",
            "commit": {
                "message": "feat: add code browser\n\nRender repository contents.",
                "author": { "name": "Yuna", "date": "2026-08-08T00:00:00Z" },
                "committer": { "name": "Yuna", "date": "2026-08-08T00:00:00Z" },
                "verification": { "verified": true }
            },
            "parents": [{ "sha": "fedcba9876543210" }],
            "author": { "login": "yuna-r" },
            "stats": { "additions": 10, "deletions": 2, "total": 12 },
            "files": [{
                "filename": "src/main.rs",
                "status": "modified",
                "additions": 10,
                "deletions": 2,
                "changes": 12,
                "patch": "@@ -1 +1 @@\n-old\n+new"
            }]
        });
        let detail: CommitDetailDto = serde_json::from_value(value).expect("valid commit response");
        let detail = detail.into_detail();
        assert_eq!(detail.summary.title, "feat: add code browser");
        assert!(detail.summary.verified);
        assert_eq!(detail.files.len(), 1);
        assert_eq!(detail.stats.total, 12);
    }
}
