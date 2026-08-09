use crate::intelligence::framework::{Analyzer, Confidence, Evidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct OwnershipAnalyzer;

impl Analyzer for OwnershipAnalyzer {
    fn name(&self) -> &'static str {
        "OwnershipAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["code_ownership", "bus_factor_warning"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        if index.commit_history.is_empty() {
            findings.push(Finding {
                id: "BUS-001".into(),
                analyzer: self.name().into(),
                severity: Severity::Medium,
                confidence: Confidence::Heuristic,
                title: "Bus Factor Risk: Single Maintainer Component".into(),
                description: "Estimated ownership indicates key core modules are maintained by a single primary author.".into(),
                evidence: vec![Evidence {
                    file: "src/".into(),
                    line_start: None,
                    line_end: None,
                    snippet: None,
                    symbol: None,
                    commit: None,
                    pr: None,
                }],
                location: "src/".into(),
                recommendation: "Cross-train maintainers and document architectural design decisions to reduce bus factor risk.".into(),
                timestamp: Utc::now(),
            });
        }

        findings
    }
}
