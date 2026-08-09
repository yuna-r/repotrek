use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Local, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::model::RateLimit;

const CACHE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_TTL_SECONDS: u64 = 15 * 60;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEventKind {
    Hit,
    Revalidated,
    Updated,
    StaleFallback,
}

impl CacheEventKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Hit => "HIT",
            Self::Revalidated => "VALIDATED",
            Self::Updated => "UPDATED",
            Self::StaleFallback => "STALE/OFFLINE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheEvent {
    kind: CacheEventKind,
    first_fetched_at: DateTime<Utc>,
    fetched_at: DateTime<Utc>,
    last_validated_at: DateTime<Utc>,
    size: u64,
    hit_count: u64,
}

impl CacheEvent {
    #[must_use]
    pub fn display_line(&self) -> String {
        format!(
            "CACHE {} · fetched {} · checked {} · first {} · {} · hits {}",
            self.kind.label(),
            local_time(&self.fetched_at),
            local_time(&self.last_validated_at),
            local_time(&self.first_fetched_at),
            human_bytes(self.size),
            self.hit_count,
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct CacheSummary {
    pub entries: usize,
    pub bytes: u64,
    pub oldest_first_fetch: Option<DateTime<Utc>>,
    pub newest_fetch: Option<DateTime<Utc>>,
    pub cache_dir: PathBuf,
}

impl CacheSummary {
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        vec![
            format!("Entries: {}", self.entries),
            format!("Stored: {}", human_bytes(self.bytes)),
            format!(
                "First cached: {}",
                self.oldest_first_fetch
                    .as_ref()
                    .map(local_date_time)
                    .unwrap_or_else(|| "-".to_owned())
            ),
            format!(
                "Last updated: {}",
                self.newest_fetch
                    .as_ref()
                    .map(local_date_time)
                    .unwrap_or_else(|| "-".to_owned())
            ),
            format!("Directory: {}", self.cache_dir.display()),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    metadata: CacheMetadata,
    body: Vec<u8>,
}

impl CacheEntry {
    #[must_use]
    pub fn body_len(&self) -> usize {
        self.body.len()
    }

    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        self.metadata.etag.as_deref()
    }

    #[must_use]
    pub fn last_modified(&self) -> Option<&str> {
        self.metadata.last_modified.as_deref()
    }

    #[must_use]
    pub fn rate_limit(&self) -> RateLimit {
        self.metadata.rate_limit.clone().into_rate_limit()
    }

    #[must_use]
    pub fn into_body(self) -> Vec<u8> {
        self.body
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    schema_version: u32,
    key: String,
    url: String,
    variant: String,
    first_fetched_at: DateTime<Utc>,
    fetched_at: DateTime<Utc>,
    last_validated_at: DateTime<Utc>,
    last_accessed_at: DateTime<Utc>,
    etag: Option<String>,
    last_modified: Option<String>,
    size: u64,
    hit_count: u64,
    rate_limit: CachedRateLimit,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CachedRateLimit {
    limit: Option<u32>,
    remaining: Option<u32>,
    reset_epoch: Option<i64>,
    resource: Option<String>,
}

impl CachedRateLimit {
    fn from_rate_limit(value: &RateLimit) -> Self {
        Self {
            limit: value.limit,
            remaining: value.remaining,
            reset_epoch: value.reset_epoch,
            resource: value.resource.clone(),
        }
    }

    fn into_rate_limit(self) -> RateLimit {
        RateLimit {
            limit: self.limit,
            remaining: self.remaining,
            reset_epoch: self.reset_epoch,
            resource: self.resource,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStore {
    inner: Arc<CacheStoreInner>,
}

#[derive(Debug)]
struct CacheStoreInner {
    cache_dir: PathBuf,
    ttl: Duration,
    state: Mutex<CacheState>,
}

#[derive(Debug, Default)]
struct CacheState {
    force_refresh: bool,
    last_event: Option<CacheEvent>,
}

impl Default for CacheStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheStore {
    #[must_use]
    pub fn new() -> Self {
        let cache_dir = ProjectDirs::from("dev", "yuna-r", "RepoTrek")
            .map(|dirs| dirs.cache_dir().join("http-v1"))
            .unwrap_or_else(|| PathBuf::from(".repotrek-cache").join("http-v1"));
        Self {
            inner: Arc::new(CacheStoreInner {
                cache_dir,
                ttl: Duration::from_secs(DEFAULT_TTL_SECONDS),
                state: Mutex::new(CacheState::default()),
            }),
        }
    }

    pub fn set_force_refresh(&self, enabled: bool) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.force_refresh = enabled;
        }
    }

    #[must_use]
    pub fn last_event(&self) -> Option<CacheEvent> {
        self.inner
            .state
            .lock()
            .ok()
            .and_then(|state| state.last_event.clone())
    }

    #[must_use]
    pub fn load(&self, url: &str, variant: &str) -> Option<CacheEntry> {
        let key = cache_key(url, variant);
        let metadata =
            serde_json::from_slice::<CacheMetadata>(&fs::read(self.metadata_path(&key)).ok()?)
                .ok()?;
        if metadata.schema_version != CACHE_SCHEMA_VERSION
            || metadata.key != key
            || metadata.url != url
            || metadata.variant != variant
        {
            return None;
        }
        let body = fs::read(self.body_path(&key)).ok()?;
        if body.len() as u64 != metadata.size {
            return None;
        }
        Some(CacheEntry { metadata, body })
    }

    #[must_use]
    pub fn is_fresh(&self, entry: &CacheEntry) -> bool {
        let force_refresh = self
            .inner
            .state
            .lock()
            .map(|state| state.force_refresh)
            .unwrap_or(false);
        if force_refresh {
            return false;
        }
        let age = Utc::now()
            .signed_duration_since(entry.metadata.last_validated_at)
            .to_std()
            .unwrap_or(Duration::ZERO);
        age <= self.inner.ttl
    }

    pub fn record_hit(&self, mut entry: CacheEntry) -> CacheEntry {
        entry.metadata.hit_count = entry.metadata.hit_count.saturating_add(1);
        entry.metadata.last_accessed_at = Utc::now();
        let _ = self.write_metadata(&entry.metadata);
        self.set_event(CacheEventKind::Hit, &entry.metadata);
        entry
    }

    pub fn record_stale_fallback(&self, mut entry: CacheEntry) -> CacheEntry {
        entry.metadata.hit_count = entry.metadata.hit_count.saturating_add(1);
        entry.metadata.last_accessed_at = Utc::now();
        let _ = self.write_metadata(&entry.metadata);
        self.set_event(CacheEventKind::StaleFallback, &entry.metadata);
        entry
    }

    pub fn revalidated(&self, mut entry: CacheEntry, rate_limit: &RateLimit) -> CacheEntry {
        let now = Utc::now();
        entry.metadata.last_validated_at = now;
        entry.metadata.last_accessed_at = now;
        entry.metadata.hit_count = entry.metadata.hit_count.saturating_add(1);
        entry.metadata.rate_limit = CachedRateLimit::from_rate_limit(rate_limit);
        let _ = self.write_metadata(&entry.metadata);
        self.set_event(CacheEventKind::Revalidated, &entry.metadata);
        entry
    }

    pub fn store(
        &self,
        url: &str,
        variant: &str,
        body: &[u8],
        etag: Option<String>,
        last_modified: Option<String>,
        rate_limit: &RateLimit,
    ) -> io::Result<()> {
        fs::create_dir_all(&self.inner.cache_dir)?;
        let key = cache_key(url, variant);
        let previous = self.load(url, variant);
        let now = Utc::now();
        let metadata = CacheMetadata {
            schema_version: CACHE_SCHEMA_VERSION,
            key: key.clone(),
            url: url.to_owned(),
            variant: variant.to_owned(),
            first_fetched_at: previous
                .as_ref()
                .map_or(now, |entry| entry.metadata.first_fetched_at),
            fetched_at: now,
            last_validated_at: now,
            last_accessed_at: now,
            etag,
            last_modified,
            size: body.len() as u64,
            hit_count: previous.map_or(0, |entry| entry.metadata.hit_count),
            rate_limit: CachedRateLimit::from_rate_limit(rate_limit),
        };
        atomic_write(&self.body_path(&key), body)?;
        self.write_metadata(&metadata)?;
        self.set_event(CacheEventKind::Updated, &metadata);
        Ok(())
    }

    pub fn clear(&self) -> io::Result<CacheSummary> {
        let before = self.summary();
        match fs::remove_dir_all(&self.inner.cache_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if let Ok(mut state) = self.inner.state.lock() {
            state.force_refresh = false;
            state.last_event = None;
        }
        Ok(before)
    }

    #[must_use]
    pub fn summary(&self) -> CacheSummary {
        let mut summary = CacheSummary {
            cache_dir: self.inner.cache_dir.clone(),
            ..CacheSummary::default()
        };
        let Ok(entries) = fs::read_dir(&self.inner.cache_dir) else {
            return summary;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let Ok(metadata) = serde_json::from_slice::<CacheMetadata>(&bytes) else {
                continue;
            };
            if metadata.schema_version != CACHE_SCHEMA_VERSION {
                continue;
            }
            summary.entries += 1;
            summary.bytes = summary.bytes.saturating_add(metadata.size);
            summary.oldest_first_fetch = Some(
                summary
                    .oldest_first_fetch
                    .map_or(metadata.first_fetched_at, |current| {
                        current.min(metadata.first_fetched_at)
                    }),
            );
            summary.newest_fetch =
                Some(summary.newest_fetch.map_or(metadata.fetched_at, |current| {
                    current.max(metadata.fetched_at)
                }));
        }
        summary
    }

    fn set_event(&self, kind: CacheEventKind, metadata: &CacheMetadata) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.last_event = Some(CacheEvent {
                kind,
                first_fetched_at: metadata.first_fetched_at,
                fetched_at: metadata.fetched_at,
                last_validated_at: metadata.last_validated_at,
                size: metadata.size,
                hit_count: metadata.hit_count,
            });
        }
    }

    fn write_metadata(&self, metadata: &CacheMetadata) -> io::Result<()> {
        fs::create_dir_all(&self.inner.cache_dir)?;
        let bytes = serde_json::to_vec_pretty(metadata).map_err(io::Error::other)?;
        atomic_write(&self.metadata_path(&metadata.key), &bytes)
    }

    fn metadata_path(&self, key: &str) -> PathBuf {
        self.inner.cache_dir.join(format!("{key}.json"))
    }

    fn body_path(&self, key: &str) -> PathBuf {
        self.inner.cache_dir.join(format!("{key}.body"))
    }
}

fn cache_key(url: &str, variant: &str) -> String {
    let input = format!("GET\n{variant}\n{url}");
    format!(
        "{:016x}{:016x}",
        fnv1a64(input.as_bytes(), 0xcbf2_9ce4_8422_2325),
        fnv1a64(input.as_bytes(), 0x8422_2325_cbf2_9ce4),
    )
}

fn fnv1a64(bytes: &[u8], seed: u64) -> u64 {
    bytes.iter().fold(seed, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!(
        "{}.{}.{sequence}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("cache"),
        std::process::id(),
    ));
    fs::write(&temporary, bytes)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_) if cfg!(windows) && path.exists() => {
            fs::remove_file(path)?;
            fs::rename(temporary, path)
        }
        Err(error) => Err(error),
    }
}

fn local_time(value: &DateTime<Utc>) -> String {
    value
        .with_timezone(&Local)
        .format("%m-%d %H:%M:%S")
        .to_string()
}

fn local_date_time(value: &DateTime<Utc>) -> String {
    value
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::{cache_key, human_bytes};

    #[test]
    fn cache_key_is_stable_and_variant_sensitive() {
        let first = cache_key("https://api.github.com/repos/a/b", "json;anonymous");
        let second = cache_key("https://api.github.com/repos/a/b", "json;anonymous");
        let authenticated = cache_key("https://api.github.com/repos/a/b", "json;auth-deadbeef");
        assert_eq!(first, second);
        assert_ne!(first, authenticated);
        assert_eq!(first.len(), 32);
    }

    #[test]
    fn byte_sizes_are_human_readable() {
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
    }
}
