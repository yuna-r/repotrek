use std::{str::FromStr, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{
    StatusCode, Url,
    blocking::{Client, RequestBuilder, Response},
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    model::{
        ApiResponse, BlameRange, BranchSummary, CodeSearchResult, CommitDetail, CommitFile,
        CommitStats, CommitSummary, ContentEntry, ContentKind, IssueSummary, PullRequestSummary,
        RateLimit, ReleaseSummary, RepoCard, Repository, RepositoryId, TreeEntry,
        WorkflowRunSummary,
    },
    provider::{ProviderError, ProviderResult, RepositoryProvider},
};

const API_BASE: &str = "https://api.github.com";
const GRAPHQL_URL: &str = "https://api.github.com/graphql";
const API_VERSION: &str = "2026-03-10";
const JSON_ACCEPT: &str = "application/vnd.github+json";
const RAW_ACCEPT: &str = "application/vnd.github.raw+json";
const MAX_TEXT_FILE_BYTES: usize = 4 * 1024 * 1024;

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
            .timeout(Duration::from_secs(40))
            .build()?;
        Ok(Self { client, token })
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    pub fn clear_token(&mut self) {
        self.token = None;
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
        T: DeserializeOwned,
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

    fn send_graphql<T>(&self, query: &str, variables: serde_json::Value) -> ProviderResult<T>
    where
        T: DeserializeOwned,
    {
        if self.token.is_none() {
            return Err(ProviderError::AuthenticationRequired {
                rate_limit: RateLimit {
                    resource: Some("graphql".to_owned()),
                    ..RateLimit::default()
                },
            });
        }

        let body = GraphQlRequest { query, variables };
        let response = self
            .request(self.client.post(GRAPHQL_URL).json(&body), JSON_ACCEPT)
            .send()?;
        let rate_limit = parse_rate_limit(response.headers());
        let response = ensure_success(response, rate_limit.clone())?;
        let envelope = response
            .json::<GraphQlEnvelope<T>>()
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        if let Some(errors) = envelope.errors {
            let message = errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(ProviderError::Api {
                status: 200,
                message,
                rate_limit,
            });
        }
        let value = envelope.data.ok_or_else(|| {
            ProviderError::InvalidResponse("GraphQL response did not contain data".to_owned())
        })?;
        Ok(ApiResponse { value, rate_limit })
    }

    fn repo_url(&self, id: &RepositoryId, endpoint: &[&str]) -> Result<Url, ProviderError> {
        let mut url = Url::parse(API_BASE)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        {
            let mut segments = url.path_segments_mut().map_err(|()| {
                ProviderError::InvalidResponse("Could not construct GitHub API URL".to_owned())
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
        let mut entries = response
            .value
            .into_iter()
            .map(ContentDto::into_content)
            .collect::<Vec<_>>();
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

    fn branches(&self, id: &RepositoryId) -> ProviderResult<Vec<BranchSummary>> {
        let url = self.repo_url(id, &["branches"])?;
        let response: ApiResponse<Vec<BranchDto>> = self.send_json(
            self.client
                .get(url)
                .query(&[("per_page", "100"), ("page", "1")]),
        )?;
        Ok(ApiResponse {
            value: response
                .value
                .into_iter()
                .map(BranchDto::into_summary)
                .collect(),
            rate_limit: response.rate_limit,
        })
    }

    fn tree(&self, id: &RepositoryId, git_ref: &str) -> ProviderResult<Vec<TreeEntry>> {
        let url = self.repo_url(id, &["git", "trees", git_ref])?;
        let response: ApiResponse<TreeResponseDto> =
            self.send_json(self.client.get(url).query(&[("recursive", "1")]))?;
        Ok(ApiResponse {
            value: response
                .value
                .tree
                .into_iter()
                .map(TreeEntryDto::into_entry)
                .collect(),
            rate_limit: response.rate_limit,
        })
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

    fn file_history(
        &self,
        id: &RepositoryId,
        git_ref: &str,
        path: &str,
        page: u32,
        per_page: u32,
    ) -> ProviderResult<Vec<CommitSummary>> {
        let url = self.repo_url(id, &["commits"])?;
        let response: ApiResponse<Vec<CommitDto>> =
            self.send_json(self.client.get(url).query(&[
                ("sha", git_ref.to_owned()),
                ("path", path.to_owned()),
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

    fn blame(
        &self,
        id: &RepositoryId,
        git_ref: &str,
        path: &str,
    ) -> ProviderResult<Vec<BlameRange>> {
        const QUERY: &str = r#"
query RepoTrekBlame($owner: String!, $name: String!, $expression: String!, $path: String!) {
  repository(owner: $owner, name: $name) {
    object(expression: $expression) {
      ... on Commit {
        blame(path: $path) {
          ranges {
            startingLine
            endingLine
            age
            commit {
              oid
              abbreviatedOid
              messageHeadline
              authoredDate
              author {
                name
                user { login }
              }
            }
          }
        }
      }
    }
  }
}
"#;
        let variables = serde_json::json!({
            "owner": id.owner.clone(),
            "name": id.name.clone(),
            "expression": git_ref,
            "path": path,
        });
        let response: ApiResponse<BlameDataDto> = self.send_graphql(QUERY, variables)?;
        let ranges = response
            .value
            .repository
            .and_then(|repository| repository.object)
            .and_then(|object| object.blame)
            .map(|blame| {
                blame
                    .ranges
                    .into_iter()
                    .map(BlameRangeDto::into_range)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(ApiResponse {
            value: ranges,
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
        let response: ApiResponse<SearchRepositoryResponseDto> =
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

    fn search_code(
        &self,
        id: &RepositoryId,
        query: &str,
        per_page: u32,
    ) -> ProviderResult<Vec<CodeSearchResult>> {
        let url = Url::parse("https://api.github.com/search/code")
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let q = format!("{query} repo:{}", id.full_name());
        let response: ApiResponse<SearchCodeResponseDto> =
            self.send_json(self.client.get(url).query(&[
                ("q", q),
                ("per_page", per_page.clamp(1, 100).to_string()),
                ("page", "1".to_owned()),
            ]))?;
        Ok(ApiResponse {
            value: response
                .value
                .items
                .into_iter()
                .map(SearchCodeItemDto::into_result)
                .collect(),
            rate_limit: response.rate_limit,
        })
    }

    fn pull_requests(&self, id: &RepositoryId) -> ProviderResult<Vec<PullRequestSummary>> {
        let url = self.repo_url(id, &["pulls"])?;
        let response: ApiResponse<Vec<PullRequestDto>> =
            self.send_json(self.client.get(url).query(&[
                ("state", "open"),
                ("sort", "updated"),
                ("per_page", "50"),
            ]))?;
        Ok(ApiResponse {
            value: response
                .value
                .into_iter()
                .map(PullRequestDto::into_summary)
                .collect(),
            rate_limit: response.rate_limit,
        })
    }

    fn issues(&self, id: &RepositoryId) -> ProviderResult<Vec<IssueSummary>> {
        let url = self.repo_url(id, &["issues"])?;
        let response: ApiResponse<Vec<IssueDto>> =
            self.send_json(self.client.get(url).query(&[
                ("state", "open"),
                ("sort", "updated"),
                ("per_page", "50"),
            ]))?;
        Ok(ApiResponse {
            value: response
                .value
                .into_iter()
                .filter(|issue| issue.pull_request.is_none())
                .map(IssueDto::into_summary)
                .collect(),
            rate_limit: response.rate_limit,
        })
    }

    fn workflow_runs(
        &self,
        id: &RepositoryId,
        git_ref: &str,
    ) -> ProviderResult<Vec<WorkflowRunSummary>> {
        let url = self.repo_url(id, &["actions", "runs"])?;
        let response: ApiResponse<WorkflowRunsResponseDto> = self.send_json(
            self.client
                .get(url)
                .query(&[("branch", git_ref), ("per_page", "50")]),
        )?;
        Ok(ApiResponse {
            value: response
                .value
                .workflow_runs
                .into_iter()
                .map(WorkflowRunDto::into_summary)
                .collect(),
            rate_limit: response.rate_limit,
        })
    }

    fn releases(&self, id: &RepositoryId) -> ProviderResult<Vec<ReleaseSummary>> {
        let url = self.repo_url(id, &["releases"])?;
        let response: ApiResponse<Vec<ReleaseDto>> = self.send_json(
            self.client
                .get(url)
                .query(&[("per_page", "50"), ("page", "1")]),
        )?;
        Ok(ApiResponse {
            value: response
                .value
                .into_iter()
                .map(ReleaseDto::into_summary)
                .collect(),
            rate_limit: response.rate_limit,
        })
    }

    fn viewer_login(&self) -> ProviderResult<String> {
        let url = Url::parse("https://api.github.com/user")
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let response: ApiResponse<ViewerDto> = self.send_json(self.client.get(url))?;
        Ok(ApiResponse {
            value: response.value.login,
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

    if status == StatusCode::UNAUTHORIZED
        || (status == StatusCode::FORBIDDEN && message.to_ascii_lowercase().contains("auth"))
    {
        return Err(ProviderError::AuthenticationRequired { rate_limit });
    }

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

#[derive(Debug, Clone, Deserialize)]
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
struct BranchDto {
    name: String,
    commit: BranchCommitDto,
    protected: bool,
}

#[derive(Debug, Deserialize)]
struct BranchCommitDto {
    sha: String,
}

impl BranchDto {
    fn into_summary(self) -> BranchSummary {
        BranchSummary {
            name: self.name,
            sha: self.commit.sha,
            protected: self.protected,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TreeResponseDto {
    tree: Vec<TreeEntryDto>,
    #[allow(dead_code)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct TreeEntryDto {
    path: String,
    sha: String,
    #[serde(rename = "type")]
    kind: String,
    size: Option<u64>,
}

impl TreeEntryDto {
    fn into_entry(self) -> TreeEntry {
        TreeEntry {
            path: self.path,
            sha: self.sha,
            kind: self.kind,
            size: self.size,
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
struct SearchRepositoryResponseDto {
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
        let id = RepositoryId::from_str(&self.full_name).unwrap_or_else(|_| RepositoryId {
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

#[derive(Debug, Deserialize)]
struct SearchCodeResponseDto {
    items: Vec<SearchCodeItemDto>,
}

#[derive(Debug, Deserialize)]
struct SearchCodeItemDto {
    name: String,
    path: String,
    sha: String,
    html_url: String,
}

impl SearchCodeItemDto {
    fn into_result(self) -> CodeSearchResult {
        CodeSearchResult {
            name: self.name,
            path: self.path,
            sha: self.sha,
            html_url: self.html_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PullRequestDto {
    number: u64,
    title: String,
    user: OwnerDto,
    head: PullRefDto,
    base: PullRefDto,
    draft: Option<bool>,
    comments: Option<u64>,
    updated_at: DateTime<Utc>,
    html_url: String,
}

#[derive(Debug, Deserialize)]
struct PullRefDto {
    #[serde(rename = "ref")]
    git_ref: String,
}

impl PullRequestDto {
    fn into_summary(self) -> PullRequestSummary {
        PullRequestSummary {
            number: self.number,
            title: self.title,
            author: self.user.login,
            head: self.head.git_ref,
            base: self.base.git_ref,
            draft: self.draft.unwrap_or(false),
            comments: self.comments.unwrap_or(0),
            updated_at: self.updated_at,
            html_url: self.html_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct IssueDto {
    number: u64,
    title: String,
    user: OwnerDto,
    comments: u64,
    labels: Vec<LabelDto>,
    updated_at: DateTime<Utc>,
    html_url: String,
    pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct LabelDto {
    name: String,
}

impl IssueDto {
    fn into_summary(self) -> IssueSummary {
        IssueSummary {
            number: self.number,
            title: self.title,
            author: self.user.login,
            comments: self.comments,
            labels: self.labels.into_iter().map(|label| label.name).collect(),
            updated_at: self.updated_at,
            html_url: self.html_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct WorkflowRunsResponseDto {
    workflow_runs: Vec<WorkflowRunDto>,
}

#[derive(Debug, Deserialize)]
struct WorkflowRunDto {
    id: u64,
    name: Option<String>,
    event: String,
    head_branch: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    created_at: DateTime<Utc>,
    html_url: String,
}

impl WorkflowRunDto {
    fn into_summary(self) -> WorkflowRunSummary {
        WorkflowRunSummary {
            id: self.id,
            name: self.name.unwrap_or_else(|| "Workflow".to_owned()),
            event: self.event,
            branch: self.head_branch.unwrap_or_default(),
            status: self.status.unwrap_or_else(|| "unknown".to_owned()),
            conclusion: self.conclusion,
            created_at: self.created_at,
            html_url: self.html_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReleaseDto {
    id: u64,
    tag_name: String,
    name: Option<String>,
    draft: bool,
    prerelease: bool,
    published_at: Option<DateTime<Utc>>,
    html_url: String,
}

impl ReleaseDto {
    fn into_summary(self) -> ReleaseSummary {
        ReleaseSummary {
            id: self.id,
            tag_name: self.tag_name,
            name: self.name,
            draft: self.draft,
            prerelease: self.prerelease,
            published_at: self.published_at,
            html_url: self.html_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ViewerDto {
    login: String,
}

#[derive(Debug, Serialize)]
struct GraphQlRequest<'a> {
    query: &'a str,
    variables: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GraphQlEnvelope<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQlErrorDto>>,
}

#[derive(Debug, Deserialize)]
struct GraphQlErrorDto {
    message: String,
}

#[derive(Debug, Deserialize)]
struct BlameDataDto {
    repository: Option<BlameRepositoryDto>,
}

#[derive(Debug, Deserialize)]
struct BlameRepositoryDto {
    object: Option<BlameObjectDto>,
}

#[derive(Debug, Deserialize)]
struct BlameObjectDto {
    blame: Option<BlameDto>,
}

#[derive(Debug, Deserialize)]
struct BlameDto {
    ranges: Vec<BlameRangeDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlameRangeDto {
    starting_line: usize,
    ending_line: usize,
    age: u8,
    commit: BlameCommitDto,
}

impl BlameRangeDto {
    fn into_range(self) -> BlameRange {
        let author = self
            .commit
            .author
            .as_ref()
            .and_then(|actor| actor.user.as_ref().map(|user| user.login.clone()))
            .or_else(|| self.commit.author.map(|actor| actor.name))
            .unwrap_or_else(|| "Unknown".to_owned());
        BlameRange {
            starting_line: self.starting_line,
            ending_line: self.ending_line,
            age: self.age,
            commit_sha: self.commit.oid,
            commit_short_sha: self.commit.abbreviated_oid,
            author,
            authored_at: self.commit.authored_date,
            message: self.commit.message_headline,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlameCommitDto {
    oid: String,
    abbreviated_oid: String,
    message_headline: String,
    authored_date: DateTime<Utc>,
    author: Option<BlameActorDto>,
}

#[derive(Debug, Deserialize)]
struct BlameActorDto {
    name: String,
    user: Option<OwnerDto>,
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
