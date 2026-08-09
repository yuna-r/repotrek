use crate::intelligence::framework::{Analyzer, Confidence, Evidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;

pub struct SecurityAnalyzer;

impl Analyzer for SecurityAnalyzer {
    fn name(&self) -> &'static str {
        "SecurityAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["secret_detection", "insecure_patterns"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        let patterns = [
            ("SEC-001", "AKIA[0-9A-Z]{16}", "AWS Access Key", Severity::High),
            ("SEC-002", "ghp_[a-zA-Z0-9]{36}", "GitHub Personal Access Token", Severity::Critical),
            ("SEC-003", "-----BEGIN PRIVATE KEY-----", "Private Key Header", Severity::Critical),
            ("SEC-004", "eyJ[a-zA-Z0-9_-]+\\.[a-zA-Z0-9_-]+\\.", "Hardcoded JWT Token", Severity::Medium),
        ];

        for (path, file) in &index.files {
            if path.contains("test") || path.contains("example") || path.contains(".md") {
                continue;
            }

            for (idx, line) in file.content_preview.lines().enumerate() {
                for (id, pat, name, sev) in &patterns {
                    if line.contains(pat) || (id == &"SEC-003" && line.contains("BEGIN PRIVATE KEY")) {
                        findings.push(Finding {
                            id: (*id).into(),
                            analyzer: self.name().into(),
                            severity: *sev,
                            confidence: Confidence::High,
                            title: format!("Potential Secret Exposure: {}", name),
                            description: format!("Discovered suspicious pattern resembling {} in {}", name, path),
                            evidence: vec![Evidence {
                                file: path.clone(),
                                line_start: Some(idx + 1),
                                line_end: Some(idx + 1),
                                snippet: Some(line.trim().to_string()),
                                symbol: None,
                                commit: None,
                                pr: None,
                            }],
                            location: format!("{}:{}", path, idx + 1),
                            recommendation: "Move sensitive credentials out of source code into environment variables or secrets manager.".into(),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        findings
    }
}
