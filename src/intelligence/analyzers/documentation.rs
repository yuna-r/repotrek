use crate::intelligence::framework::{Analyzer, Confidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct DocumentationAnalyzer;

impl Analyzer for DocumentationAnalyzer {
    fn name(&self) -> &'static str {
        "DocumentationAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["doc_health_check"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        let has_readme = index.files.keys().any(|p| p.to_lowercase().starts_with("readme"));
        let has_license = index.files.keys().any(|p| p.to_lowercase().starts_with("license"));
        let has_changelog = index.files.keys().any(|p| p.to_lowercase().starts_with("changelog"));

        if !has_readme {
            findings.push(Finding {
                id: "DOC-001".into(),
                analyzer: self.name().into(),
                severity: Severity::Medium,
                confidence: Confidence::Confirmed,
                title: "Missing README File".into(),
                description: "Repository lacks a top-level README.md documentation file.".into(),
                evidence: vec![],
                location: "Repository Root".into(),
                recommendation: "Create a README.md file outlining project usage and architecture.".into(),
                timestamp: Utc::now(),
            });
        }

        if !has_license {
            findings.push(Finding {
                id: "DOC-002".into(),
                analyzer: self.name().into(),
                severity: Severity::Low,
                confidence: Confidence::Confirmed,
                title: "Missing LICENSE File".into(),
                description: "No explicit LICENSE file found in repository root.".into(),
                evidence: vec![],
                location: "Repository Root".into(),
                recommendation: "Add an open source or proprietary license file.".into(),
                timestamp: Utc::now(),
            });
        }

        if !has_changelog {
            findings.push(Finding {
                id: "DOC-003".into(),
                analyzer: self.name().into(),
                severity: Severity::Info,
                confidence: Confidence::Confirmed,
                title: "Missing CHANGELOG File".into(),
                description: "No CHANGELOG file found to track release updates.".into(),
                evidence: vec![],
                location: "Repository Root".into(),
                recommendation: "Maintain a CHANGELOG.md to communicate version releases.".into(),
                timestamp: Utc::now(),
            });
        }

        findings
    }
}
