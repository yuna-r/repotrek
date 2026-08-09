use crate::intelligence::framework::{Analyzer, Confidence, Evidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct CiAnalyzer;

impl Analyzer for CiAnalyzer {
    fn name(&self) -> &'static str {
        "CiAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["workflow_analytics", "failure_correlation"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        let workflow_files: Vec<_> = index.files.keys().filter(|p| p.starts_with(".github/workflows/")).collect();
        if workflow_files.is_empty() {
            findings.push(Finding {
                id: "CI-001".into(),
                analyzer: self.name().into(),
                severity: Severity::Low,
                confidence: Confidence::Confirmed,
                title: "CI/CD Workflows Not Configured".into(),
                description: "No GitHub Actions workflows found in .github/workflows/.".into(),
                evidence: vec![],
                location: ".github/workflows/".into(),
                recommendation: "Create CI workflows for automated building, linting, and testing.".into(),
                timestamp: Utc::now(),
            });
        } else {
            findings.push(Finding {
                id: "CI-002".into(),
                analyzer: self.name().into(),
                severity: Severity::Info,
                confidence: Confidence::Confirmed,
                title: format!("Active CI/CD Workflows: {}", workflow_files.len()),
                description: format!("Discovered {} workflow configurations.", workflow_files.len()),
                evidence: workflow_files
                    .iter()
                    .map(|w| Evidence {
                        file: (*w).clone(),
                        line_start: Some(1),
                        line_end: None,
                        snippet: None,
                        symbol: None,
                        commit: None,
                        pr: None,
                    })
                    .collect(),
                location: ".github/workflows/".into(),
                recommendation: "Monitor workflow step durations to prevent build bottlenecks.".into(),
                timestamp: Utc::now(),
            });
        }

        findings
    }
}
