use crate::intelligence::framework::{Finding, Severity};
use crate::intelligence::index::RepoIndex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryScore {
    pub name: String,
    pub score: u8,
    pub findings_count: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthScore {
    pub overall: u8,
    pub status: String,
    pub categories: Vec<CategoryScore>,
    pub total_findings: usize,
    pub critical_count: usize,
    pub high_count: usize,
}

pub struct HealthCalculator;

impl HealthCalculator {
    pub fn calculate(index: &RepoIndex, findings: &[Finding]) -> HealthScore {
        let mut critical_count = 0;
        let mut high_count = 0;
        let mut medium_count = 0;
        let mut _low_count = 0;

        for f in findings {
            match f.severity {
                Severity::Critical => critical_count += 1,
                Severity::High => high_count += 1,
                Severity::Medium => medium_count += 1,
                Severity::Low => _low_count += 1,
                Severity::Info => {}
            }
        }

        let mut sec_score: i32 = 100 - (critical_count as i32 * 25) - (high_count as i32 * 10) - (medium_count as i32 * 3);
        sec_score = sec_score.clamp(0, 100);

        let has_readme = index.files.keys().any(|p| p.to_lowercase().starts_with("readme"));
        let doc_score = if has_readme { 85 } else { 45 };

        let dep_count = index.dependencies.len();
        let dep_score = if dep_count > 50 { 70 } else { 90 };

        let qual_score = if index.total_loc > 50_000 { 75 } else { 88 };
        let arch_score = if index.files.values().any(|f| f.path.contains("src/")) { 85 } else { 70 };

        let overall = (sec_score * 3 + doc_score * 1 + dep_score * 2 + qual_score * 2 + arch_score * 2) / 10;
        let overall_u8 = overall.clamp(0, 100) as u8;

        let status = if overall_u8 >= 85 {
            "EXCELLENT"
        } else if overall_u8 >= 70 {
            "GOOD"
        } else if overall_u8 >= 50 {
            "NEEDS IMPROVEMENT"
        } else {
            "CRITICAL ATTENTION"
        };

        HealthScore {
            overall: overall_u8,
            status: status.to_string(),
            categories: vec![
                CategoryScore {
                    name: "Security".into(),
                    score: sec_score as u8,
                    findings_count: critical_count + high_count,
                    status: if sec_score >= 80 { "SECURE" } else { "VULNERABLE" }.into(),
                },
                CategoryScore {
                    name: "Architecture".into(),
                    score: arch_score as u8,
                    findings_count: 0,
                    status: "STRUCTURED".into(),
                },
                CategoryScore {
                    name: "Dependencies".into(),
                    score: dep_score as u8,
                    findings_count: 0,
                    status: "MANAGED".into(),
                },
                CategoryScore {
                    name: "Quality".into(),
                    score: qual_score as u8,
                    findings_count: medium_count,
                    status: "MAINTAINABLE".into(),
                },
                CategoryScore {
                    name: "Documentation".into(),
                    score: doc_score as u8,
                    findings_count: 0,
                    status: if has_readme { "COMPLETE" } else { "MISSING" }.into(),
                },
            ],
            total_findings: findings.len(),
            critical_count,
            high_count,
        }
    }
}
