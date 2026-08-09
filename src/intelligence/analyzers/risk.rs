use crate::intelligence::framework::{Analyzer, Confidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct RiskAnalyzer;

impl Analyzer for RiskAnalyzer {
    fn name(&self) -> &'static str {
        "RiskAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["change_risk_analysis", "pr_impact"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();
        let test_files_count = index.files.keys().filter(|p| p.contains("test")).count();
        if test_files_count == 0 {
            findings.push(Finding {
                id: "RISK-001".into(),
                analyzer: self.name().into(),
                severity: Severity::Medium,
                confidence: Confidence::Confirmed,
                title: "Change Risk Elevated: Missing Automated Test Suite".into(),
                description: "No dedicated test directory or files detected. Code changes carry higher regression risk.".into(),
                evidence: vec![],
                location: "Repository Root".into(),
                recommendation: "Add unit tests to validate core features during pull requests.".into(),
                timestamp: Utc::now(),
            });
        }
        findings
    }
}
