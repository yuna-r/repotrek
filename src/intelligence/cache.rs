use crate::intelligence::framework::Finding;
use crate::intelligence::index::RepoIndex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAnalysis {
    pub repo_name: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub index: RepoIndex,
    pub findings: Vec<Finding>,
}

pub struct CacheManager {
    cache_dir: PathBuf,
}

impl CacheManager {
    pub fn new() -> Self {
        let base_dir = directories::ProjectDirs::from("com", "repotrek", "RepoTrek")
            .map(|p| p.cache_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".repotrek_cache"));
        Self { cache_dir: base_dir }
    }

    pub fn save(&self, repo_name: &str, index: &RepoIndex, findings: &[Finding]) -> anyhow::Result<()> {
        fs::create_dir_all(&self.cache_dir)?;
        let safe_name = repo_name.replace('/', "_");
        let file_path = self.cache_dir.join(format!("{}.json", safe_name));

        let cache = CachedAnalysis {
            repo_name: repo_name.to_string(),
            timestamp: chrono::Utc::now(),
            index: index.clone(),
            findings: findings.to_vec(),
        };

        let json = serde_json::to_string_pretty(&cache)?;
        fs::write(file_path, json)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn load(&self, repo_name: &str) -> anyhow::Result<CachedAnalysis> {
        let safe_name = repo_name.replace('/', "_");
        let file_path = self.cache_dir.join(format!("{}.json", safe_name));
        let content = fs::read_to_string(file_path)?;
        let cache: CachedAnalysis = serde_json::from_str(&content)?;
        Ok(cache)
    }

    #[allow(dead_code)]
    pub fn exists(&self, repo_name: &str) -> bool {
        let safe_name = repo_name.replace('/', "_");
        let file_path = self.cache_dir.join(format!("{}.json", safe_name));
        file_path.exists()
    }
}
