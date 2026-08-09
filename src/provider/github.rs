use std::{str::FromStr, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{
    StatusCode, Url,
    blocking::{Client, RequestBuilder, Response},
    header::{
        ACCEPT, AUTHORIZATION, ETAG, HeaderMap, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH,
        LAST_MODIFIED, RETRY_AFTER,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    cache::{CacheStore, CacheSummary},
    model::{
        ApiResponse, BlameRange, BranchSummary, CodeSearchResult, Comment, CommitDetail,
        CommitFile, CommitStats, CommitSummary, ContentEntry, ContentKind, IssueDetail,
        IssueSummary, OpenClosedFilter, PullRequestDetail, PullRequestSummary, RateLimit,
        ReleaseAsset, ReleaseDetail, ReleaseSummary, RepoCard, Repository, RepositoryId, TreeEntry,
        WorkflowJob, WorkflowRunDetail, WorkflowRunSummary, WorkflowStep,
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
    cache: CacheStore,
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
        Ok(Self {
            client,
            token,
            cache: CacheStore::new(),
        })
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

    pub fn set_force_refresh(&self, enabled: bool) {
        self.cache.set_force_refresh(enabled);
    }

    #[must_use]
    pub fn cache_status_line(&self) -> Option<String> {
        self.cache.last_event().map(|event| event.display_line())
    }

    #[must_use]
    pub fn cache_summary(&self) -> CacheSummary {
        self.cache.summary()
    }

    pub fn clear_cache(&self) -> std::io::Result<CacheSummary> {
        self.cache.clear()
    }

    fn cache_variant(&self, accept: &str) -> String {
        self.token.as_ref().map_or_else(
            || format!("{accept};scope=anonymous"),
            |token| format!("{accept};scope=auth-{:016x}", token_fingerprint(token)),
        )
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
        let (bytes, rate_limit) = self.send_cached_bytes(request, JSON_ACCEPT, None)?;
        let value = serde_json::from_slice::<T>(&bytes)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        Ok(ApiResponse { value, rate_limit })
    }

    fn send_json_uncached<T>(&self, request: RequestBuilder) -> ProviderResult<T>
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

    fn send_cached_bytes(
        &self,
        request: RequestBuilder,
        accept: &'static str,
        max_bytes: Option<usize>,
    ) -> Result<(Vec<u8>, RateLimit), ProviderError> {
        let mut request = self.request(request, accept);
        let request_url = request
            .try_clone()
            .and_then(|clone| clone.build().ok())
            .map(|request| request.url().to_string());
        let variant = self.cache_variant(accept);
        let cached = request_url
            .as_deref()
            .and_then(|url| self.cache.load(url, &variant));

        if let Some(limit) = max_bytes
            && let Some(entry) = cached.as_ref()
            && entry.body_len() > limit
        {
            return Err(ProviderError::FileTooLarge {
                size: entry.body_len(),
                limit,
            });
        }

        if let Some(entry) = cached.as_ref()
            && self.cache.is_fresh(entry)
        {
            let entry = self.cache.record_hit(entry.clone());
            let rate_limit = entry.rate_limit();
            return Ok((entry.into_body(), rate_limit));
        }

        if let Some(entry) = cached.as_ref() {
            if let Some(etag) = entry.etag() {
                request = request.header(IF_NONE_MATCH, etag);
            }
            if let Some(last_modified) = entry.last_modified() {
                request = request.header(IF_MODIFIED_SINCE, last_modified);
            }
        }

        let response = match request.send() {
            Ok(response) => response,
            Err(error) => {
                if let Some(entry) = cached {
                    let entry = self.cache.record_stale_fallback(entry);
                    let rate_limit = entry.rate_limit();
                    return Ok((entry.into_body(), rate_limit));
                }
                return Err(error.into());
            }
        };
        let rate_limit = parse_rate_limit(response.headers());

        if response.status().is_server_error()
            && let Some(entry) = cached.as_ref()
        {
            let entry = self.cache.record_stale_fallback(entry.clone());
            let cached_rate_limit = entry.rate_limit();
            return Ok((entry.into_body(), cached_rate_limit));
        }

        if response.status() == StatusCode::NOT_MODIFIED {
            let Some(entry) = cached else {
                return Err(ProviderError::InvalidResponse(
                    "GitHub returned 304 without a local cache entry".to_owned(),
                ));
            };
            let effective_rate_limit = if rate_limit_present(&rate_limit) {
                rate_limit
            } else {
                entry.rate_limit()
            };
            let entry = self.cache.revalidated(entry, &effective_rate_limit);
            return Ok((entry.into_body(), effective_rate_limit));
        }

        let response = ensure_success(response, rate_limit.clone())?;
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let last_modified = response
            .headers()
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let bytes = response.bytes()?.to_vec();
        if let Some(limit) = max_bytes
            && bytes.len() > limit
        {
            return Err(ProviderError::FileTooLarge {
                size: bytes.len(),
                limit,
            });
        }
        if let Some(url) = request_url.as_deref() {
            let _ = self
                .cache
                .store(url, &variant, &bytes, etag, last_modified, &rate_limit);
        }
        Ok((bytes, rate_limit))
    }

    /// Retry a public GitHub REST request once without Authorization when a
    /// fine-grained token hides an otherwise public resource behind HTTP 404.
    fn send_json_public_fallback<T>(&self, request: RequestBuilder) -> ProviderResult<T>
    where
        T: DeserializeOwned,
    {
        let anonymous_request = request.try_clone();

        match self.send_json(request) {
            Err(original @ ProviderError::Api { status: 404, .. }) if self.token.is_some() => {
                let Some(anonymous_request) = anonymous_request else {
                    return Err(original);
                };

                // Deliberately do not call self.request() here: it would attach
                // the stored Authorization header again.
                let response = anonymous_request
                    .header(ACCEPT, JSON_ACCEPT)
                    .header("X-GitHub-Api-Version", API_VERSION)
                    .send()?;
                let rate_limit = parse_rate_limit(response.headers());
                let response = match ensure_success(response, rate_limit.clone()) {
                    Ok(response) => response,
                    // Preserve the authenticated 404 if GitHub also returns 404
                    // anonymously. pull_requests() can then treat that as an
                    // unavailable/restricted PR surface.
                    Err(ProviderError::Api { status: 404, .. }) => return Err(original),
                    Err(error) => return Err(error),
                };
                let value = response
                    .json::<T>()
                    .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
                Ok(ApiResponse { value, rate_limit })
            }
            result => result,
        }
    }

    fn send_text(&self, request: RequestBuilder) -> ProviderResult<String> {
        let (bytes, rate_limit) =
            self.send_cached_bytes(request, RAW_ACCEPT, Some(MAX_TEXT_FILE_BYTES))?;
        let value = String::from_utf8(bytes).map_err(|_| ProviderError::BinaryFile)?;
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
        let mut request = self.client.get(url).query(&[
            ("q", query.to_owned()),
            ("per_page", per_page.clamp(1, 30).to_string()),
        ]);
        if !sort.is_empty() && sort != "best-match" {
            request = request.query(&[("sort", sort), ("order", "desc")]);
        }
        let response: ApiResponse<SearchRepositoryResponseDto> = self.send_json(request)?;
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
        if self.token.is_none() {
            return Err(ProviderError::AuthenticationRequired {
                rate_limit: RateLimit {
                    resource: Some("search".to_owned()),
                    ..RateLimit::default()
                },
            });
        }
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

    fn pull_requests(
        &self,
        id: &RepositoryId,
        state: OpenClosedFilter,
    ) -> ProviderResult<Vec<PullRequestSummary>> {
        let url = self.repo_url(id, &["pulls"])?;
        let result: ProviderResult<Vec<PullRequestDto>> =
            self.send_json_public_fallback(self.client.get(url).query(&[
                ("state", state.api_value()),
                ("sort", "updated"),
                ("direction", "desc"),
                ("per_page", "100"),
            ]));

        match result {
            Ok(response) => Ok(ApiResponse {
                value: response
                    .value
                    .into_iter()
                    .map(PullRequestDto::into_summary)
                    .collect(),
                rate_limit: response.rate_limit,
            }),
            // Some public repositories intentionally do not expose pull requests.
            // In that case GitHub returns 404 even though the repository itself is readable.
            Err(ProviderError::Api {
                status: 404,
                rate_limit,
                ..
            }) => Ok(ApiResponse {
                value: Vec::new(),
                rate_limit,
            }),
            Err(error) => Err(error),
        }
    }

    fn pull_request(&self, id: &RepositoryId, number: u64) -> ProviderResult<PullRequestDetail> {
        let number_text = number.to_string();

        // The pull request itself is mandatory. Supplementary resources such as
        // changed files and conversation comments are loaded best-effort below so
        // that an unavailable secondary endpoint does not make the whole PR unreadable.
        let url = self.repo_url(id, &["pulls", &number_text])?;
        let response: ApiResponse<PullRequestDto> = self.send_json(self.client.get(url))?;
        let rate_limit = response.rate_limit.clone();
        let summary = response.value.clone().into_summary();
        let detail = response.value;

        let files_url = self.repo_url(id, &["pulls", &number_text, "files"])?;
        let files = match self.send_json::<Vec<CommitFileDto>>(
            self.client
                .get(files_url)
                .query(&[("per_page", "100"), ("page", "1")]),
        ) {
            Ok(response) => response
                .value
                .into_iter()
                .map(CommitFileDto::into_file)
                .collect(),
            Err(ProviderError::Api { status: 404, .. }) => Vec::new(),
            Err(error) => return Err(error),
        };

        let comments_url = self.repo_url(id, &["issues", &number_text, "comments"])?;
        let comments = match self.send_json::<Vec<CommentDto>>(
            self.client
                .get(comments_url)
                .query(&[("per_page", "100"), ("page", "1")]),
        ) {
            Ok(response) => response
                .value
                .into_iter()
                .map(CommentDto::into_comment)
                .collect(),
            Err(ProviderError::Api { status: 404, .. }) => Vec::new(),
            Err(error) => return Err(error),
        };

        Ok(ApiResponse {
            value: PullRequestDetail {
                summary,
                state: detail.state.unwrap_or_else(|| "unknown".to_owned()),
                merged: detail.merged.unwrap_or(false) || detail.merged_at.is_some(),
                body: detail.body.unwrap_or_default(),
                commits: detail.commits.unwrap_or(0),
                changed_files: detail.changed_files.unwrap_or(0),
                additions: detail.additions.unwrap_or(0),
                deletions: detail.deletions.unwrap_or(0),
                files,
                comments,
            },
            rate_limit,
        })
    }

    fn issues(
        &self,
        id: &RepositoryId,
        state: OpenClosedFilter,
    ) -> ProviderResult<Vec<IssueSummary>> {
        let url = self.repo_url(id, &["issues"])?;
        let response: ApiResponse<Vec<IssueDto>> =
            self.send_json(self.client.get(url).query(&[
                ("state", state.api_value()),
                ("sort", "updated"),
                ("direction", "desc"),
                ("per_page", "100"),
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

    fn issue(&self, id: &RepositoryId, number: u64) -> ProviderResult<IssueDetail> {
        let number_text = number.to_string();
        let url = self.repo_url(id, &["issues", &number_text])?;
        let response: ApiResponse<IssueDto> = self.send_json(self.client.get(url))?;
        let rate_limit = response.rate_limit.clone();
        let summary = response.value.clone().into_summary();
        let detail = response.value;

        let comments_url = self.repo_url(id, &["issues", &number_text, "comments"])?;
        let comments_response: ApiResponse<Vec<CommentDto>> = self.send_json(
            self.client
                .get(comments_url)
                .query(&[("per_page", "100"), ("page", "1")]),
        )?;

        Ok(ApiResponse {
            value: IssueDetail {
                summary,
                state: detail.state.unwrap_or_else(|| "unknown".to_owned()),
                body: detail.body.unwrap_or_default(),
                comments: comments_response
                    .value
                    .into_iter()
                    .map(CommentDto::into_comment)
                    .collect(),
            },
            rate_limit,
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

    fn workflow_run(&self, id: &RepositoryId, run_id: u64) -> ProviderResult<WorkflowRunDetail> {
        let run_id_text = run_id.to_string();
        let url = self.repo_url(id, &["actions", "runs", &run_id_text])?;
        let response: ApiResponse<WorkflowRunDto> = self.send_json(self.client.get(url))?;
        let rate_limit = response.rate_limit.clone();
        let summary = response.value.into_summary();

        let jobs_url = self.repo_url(id, &["actions", "runs", &run_id_text, "jobs"])?;
        let jobs_response: ApiResponse<WorkflowJobsResponseDto> = self.send_json(
            self.client
                .get(jobs_url)
                .query(&[("per_page", "100"), ("page", "1")]),
        )?;

        Ok(ApiResponse {
            value: WorkflowRunDetail {
                summary,
                jobs: jobs_response
                    .value
                    .jobs
                    .into_iter()
                    .map(WorkflowJobDto::into_job)
                    .collect(),
            },
            rate_limit,
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

    fn release(&self, id: &RepositoryId, release_id: u64) -> ProviderResult<ReleaseDetail> {
        let release_id_text = release_id.to_string();
        let url = self.repo_url(id, &["releases", &release_id_text])?;
        let response: ApiResponse<ReleaseDto> = self.send_json(self.client.get(url))?;
        let rate_limit = response.rate_limit.clone();
        let detail = response.value;
        let summary = detail.clone().into_summary();
        Ok(ApiResponse {
            value: ReleaseDetail {
                summary,
                author: detail
                    .author
                    .map_or_else(|| "Unknown".to_owned(), |author| author.login),
                body: detail.body.unwrap_or_default(),
                assets: detail
                    .assets
                    .unwrap_or_default()
                    .into_iter()
                    .map(ReleaseAssetDto::into_asset)
                    .collect(),
            },
            rate_limit,
        })
    }

    fn viewer_login(&self) -> ProviderResult<String> {
        let url = Url::parse("https://api.github.com/user")
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let response: ApiResponse<ViewerDto> = self.send_json_uncached(self.client.get(url))?;
        Ok(ApiResponse {
            value: response.value.login,
            rate_limit: response.rate_limit,
        })
    }
}

fn token_fingerprint(token: &str) -> u64 {
    token
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn rate_limit_present(rate_limit: &RateLimit) -> bool {
    rate_limit.limit.is_some()
        || rate_limit.remaining.is_some()
        || rate_limit.reset_epoch.is_some()
        || rate_limit.resource.is_some()
}

fn ensure_success(response: Response, rate_limit: RateLimit) -> Result<Response, ProviderError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let request_url = response.url().to_string();
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
        if rate_limit.exhausted() {
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
        message: format!("{message} [{request_url}]"),
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
            line: None,
            preview: None,
            kind: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
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
    state: Option<String>,
    merged: Option<bool>,
    merged_at: Option<DateTime<Utc>>,
    body: Option<String>,
    commits: Option<u64>,
    changed_files: Option<u64>,
    additions: Option<u64>,
    deletions: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
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
            state: self.state.unwrap_or_else(|| "open".to_owned()),
            merged: self.merged.unwrap_or(false) || self.merged_at.is_some(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct IssueDto {
    number: u64,
    title: String,
    user: OwnerDto,
    comments: u64,
    labels: Vec<LabelDto>,
    updated_at: DateTime<Utc>,
    html_url: String,
    pull_request: Option<serde_json::Value>,
    state: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
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
            state: self.state.unwrap_or_else(|| "open".to_owned()),
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

#[derive(Debug, Clone, Deserialize)]
struct ReleaseDto {
    id: u64,
    tag_name: String,
    name: Option<String>,
    draft: bool,
    prerelease: bool,
    published_at: Option<DateTime<Utc>>,
    html_url: String,
    author: Option<OwnerDto>,
    body: Option<String>,
    assets: Option<Vec<ReleaseAssetDto>>,
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
struct CommentDto {
    id: u64,
    user: OwnerDto,
    body: Option<String>,
    created_at: DateTime<Utc>,
    html_url: String,
}

impl CommentDto {
    fn into_comment(self) -> Comment {
        Comment {
            id: self.id,
            author: self.user.login,
            body: self.body.unwrap_or_default(),
            created_at: self.created_at,
            html_url: self.html_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct WorkflowJobsResponseDto {
    jobs: Vec<WorkflowJobDto>,
}

#[derive(Debug, Deserialize)]
struct WorkflowJobDto {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: String,
    steps: Option<Vec<WorkflowStepDto>>,
}

impl WorkflowJobDto {
    fn into_job(self) -> WorkflowJob {
        WorkflowJob {
            id: self.id,
            name: self.name,
            status: self.status,
            conclusion: self.conclusion,
            html_url: self.html_url,
            steps: self
                .steps
                .unwrap_or_default()
                .into_iter()
                .map(WorkflowStepDto::into_step)
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct WorkflowStepDto {
    number: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
}

impl WorkflowStepDto {
    fn into_step(self) -> WorkflowStep {
        WorkflowStep {
            number: self.number,
            name: self.name,
            status: self.status,
            conclusion: self.conclusion,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseAssetDto {
    id: u64,
    name: String,
    size: u64,
    download_count: u64,
    browser_download_url: String,
}

impl ReleaseAssetDto {
    fn into_asset(self) -> ReleaseAsset {
        ReleaseAsset {
            id: self.id,
            name: self.name,
            size: self.size,
            download_count: self.download_count,
            browser_download_url: self.browser_download_url,
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
    use super::{CommitDetailDto, ContentDto, IssueDto, PullRequestDto, RepositoryDto};

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

    #[test]
    fn parses_closed_merged_pull_request_summary() {
        let value = serde_json::json!({
            "number": 42,
            "title": "Add cancellable search",
            "user": { "login": "contributor" },
            "head": { "ref": "feature/cancel" },
            "base": { "ref": "main" },
            "draft": false,
            "comments": 3,
            "updated_at": "2026-08-09T00:00:00Z",
            "html_url": "https://github.com/yuna-r/repotrek/pull/42",
            "state": "closed",
            "merged_at": "2026-08-09T01:00:00Z"
        });
        let summary = serde_json::from_value::<PullRequestDto>(value)
            .expect("valid pull request response")
            .into_summary();
        assert_eq!(summary.state, "closed");
        assert!(summary.merged);
    }

    #[test]
    fn parses_closed_issue_summary() {
        let value = serde_json::json!({
            "number": 7,
            "title": "Document state filters",
            "user": { "login": "reporter" },
            "comments": 1,
            "labels": [{ "name": "documentation" }],
            "updated_at": "2026-08-09T00:00:00Z",
            "html_url": "https://github.com/yuna-r/repotrek/issues/7",
            "state": "closed"
        });
        let summary = serde_json::from_value::<IssueDto>(value)
            .expect("valid issue response")
            .into_summary();
        assert_eq!(summary.state, "closed");
        assert_eq!(summary.labels, vec!["documentation".to_owned()]);
    }
}
