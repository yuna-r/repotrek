use crate::intelligence::framework::{Analyzer, Confidence, Evidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct ArchitectureAnalyzer;

impl Analyzer for ArchitectureAnalyzer {
    fn name(&self) -> &'static str {
        "ArchitectureAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["entry_points", "layered_architecture", "module_decomposition"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        let mut entry_points = Vec::new();
        for (path, file) in &index.files {
            if path == "src/main.rs"
                || path == "src/lib.rs"
                || path == "index.js"
                || path == "index.ts"
                || path == "app.py"
                || path == "main.go"
            {
                entry_points.push(file);
            }
        }

        if !entry_points.is_empty() {
            let evidence = entry_points
                .iter()
                .map(|e| Evidence {
                    file: e.path.clone(),
                    line_start: Some(1),
                    line_end: Some(e.line_count.min(20)),
                    snippet: Some(e.content_preview.clone()),
                    symbol: None,
                    commit: None,
                    pr: None,
                })
                .collect();

            findings.push(Finding {
                id: "ARCH-001".into(),
                analyzer: self.name().into(),
                severity: Severity::Info,
                confidence: Confidence::Confirmed,
                title: format!("Discovered {} Entry Points", entry_points.len()),
                description: format!(
                    "Primary application entry points detected: {}",
                    entry_points
                        .iter()
                        .map(|e| e.path.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                evidence,
                location: "Repository Root".into(),
                recommendation: "Ensure entry points separate CLI/HTTP delivery from core business logic.".into(),
                timestamp: Utc::now(),
            });
        }

        findings
    }
}
