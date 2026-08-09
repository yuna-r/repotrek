use crate::intelligence::framework::{Analyzer, Confidence, Evidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct HotspotAnalyzer;

impl Analyzer for HotspotAnalyzer {
    fn name(&self) -> &'static str {
        "HotspotAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["code_hotspot_analysis", "churn_complexity_correlation"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        for (path, file) in &index.files {
            if (path.contains("app") || path.contains("ui") || path.contains("main")) && file.line_count > 500 {
                findings.push(Finding {
                    id: "HOTSPOT-001".into(),
                    analyzer: self.name().into(),
                    severity: Severity::Medium,
                    confidence: Confidence::Medium,
                    title: format!("High Risk Code Hotspot: {}", path),
                    description: format!("File {} combines high LOC ({}) and core application responsibilities.", path, file.line_count),
                    evidence: vec![Evidence {
                        file: path.clone(),
                        line_start: Some(1),
                        line_end: Some(file.line_count),
                        snippet: None,
                        symbol: None,
                        commit: None,
                        pr: None,
                    }],
                    location: path.clone(),
                    recommendation: "Prioritize automated testing and code reviews for this hotspot.".into(),
                    timestamp: Utc::now(),
                });
            }
        }

        findings
    }
}
