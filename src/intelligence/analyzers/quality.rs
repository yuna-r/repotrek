use crate::intelligence::framework::{Analyzer, Confidence, Evidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct QualityAnalyzer;

impl Analyzer for QualityAnalyzer {
    fn name(&self) -> &'static str {
        "QualityAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["cyclomatic_complexity", "file_length_analysis", "nesting_depth"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        for (path, file) in &index.files {
            if file.line_count > 1000 {
                findings.push(Finding {
                    id: "QUAL-001".into(),
                    analyzer: self.name().into(),
                    severity: Severity::Medium,
                    confidence: Confidence::Confirmed,
                    title: format!("High LOC File: {}", path),
                    description: format!("File contains {} lines of code (exceeds 1,000 threshold).", file.line_count),
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
                    recommendation: "Decompose large source files into modular subcomponents to enhance maintainability.".into(),
                    timestamp: Utc::now(),
                });
            }

            let mut decision_points = 0;
            for line in file.content_preview.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("if ")
                    || trimmed.starts_with("else if")
                    || trimmed.starts_with("for ")
                    || trimmed.starts_with("while ")
                    || trimmed.contains("match ")
                {
                    decision_points += 1;
                }
            }

            if decision_points > 15 {
                findings.push(Finding {
                    id: "QUAL-002".into(),
                    analyzer: self.name().into(),
                    severity: Severity::Low,
                    confidence: Confidence::Heuristic,
                    title: format!("High Complexity File: {}", path),
                    description: format!("File contains high branching density ({} decision keywords).", decision_points),
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
                    recommendation: "Refactor complex branching into strategy pattern or helper functions.".into(),
                    timestamp: Utc::now(),
                });
            }
        }

        findings
    }
}
