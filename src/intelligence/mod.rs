pub mod ai;
pub mod analyzers;
pub mod cache;
pub mod framework;
pub mod health;
pub mod index;
pub mod mcp;
pub mod reporting;

pub use ai::{AiGateway, PrivacyMode};
pub use cache::CacheManager;
pub use framework::Finding;
pub use health::{HealthCalculator, HealthScore};
pub use index::RepoIndex;
pub use mcp::McpServer;
pub use reporting::{format_report, ReportFormat, RepositoryReport};

pub struct IntelligenceEngine {
    cache_manager: CacheManager,
}

impl IntelligenceEngine {
    pub fn new() -> Self {
        Self {
            cache_manager: CacheManager::new(),
        }
    }

    pub fn analyze_local(&self, root_path: &std::path::Path) -> anyhow::Result<(RepoIndex, Vec<Finding>, HealthScore)> {
        let repo_name = root_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut index = RepoIndex::new(repo_name.clone());
        index.scan_local_directory(root_path)?;

        let findings = analyzers::run_all_analyzers(&index);
        let health = HealthCalculator::calculate(&index, &findings);

        let _ = self.cache_manager.save(&repo_name, &index, &findings);

        Ok((index, findings, health))
    }
}
