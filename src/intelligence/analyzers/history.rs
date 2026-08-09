use crate::intelligence::framework::{Analyzer, Confidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct HistoryAnalyzer;

impl Analyzer for HistoryAnalyzer {
    fn name(&self) -> &'static str {
        "HistoryAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["commit_velocity", "file_churn"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();
        if !index.commit_history.is_empty() {
            findings.push(Finding {
                id: "HIST-001".into(),
                analyzer: self.name().into(),
                severity: Severity::Info,
                confidence: Confidence::Confirmed,
                title: format!("Indexed {} Commits in History", index.commit_history.len()),
                description: format!("Analyzed git evolution across {} commits.", index.commit_history.len()),
                evidence: vec![],
                location: "Git Log".into(),
                recommendation: "Maintain clear semantic commit messages for automated changelog generation.".into(),
                timestamp: Utc::now(),
            });
        }
        findings
    }
}
