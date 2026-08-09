use crate::intelligence::framework::{Analyzer, Confidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct ApiAnalyzer;

impl Analyzer for ApiAnalyzer {
    fn name(&self) -> &'static str {
        "ApiAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["api_surface_inventory"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        let mut public_functions = 0;
        for file in index.files.values() {
            for line in file.content_preview.lines() {
                if line.trim().starts_with("pub fn ") || line.trim().starts_with("pub async fn ") {
                    public_functions += 1;
                }
            }
        }

        if public_functions > 0 {
            findings.push(Finding {
                id: "API-001".into(),
                analyzer: self.name().into(),
                severity: Severity::Info,
                confidence: Confidence::Confirmed,
                title: format!("Exported Public Functions: {}", public_functions),
                description: format!("Discovered {} public function declarations across source files.", public_functions),
                evidence: vec![],
                location: "src/".into(),
                recommendation: "Ensure public API contracts are documented with docstrings.".into(),
                timestamp: Utc::now(),
            });
        }

        findings
    }
}
