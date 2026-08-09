use crate::intelligence::framework::Finding;
use crate::intelligence::health::HealthCalculator;
use crate::intelligence::index::RepoIndex;
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Json,
    Markdown,
    Html,
    Sarif,
}

pub fn format_report(format: ReportFormat, index: &RepoIndex, findings: &[Finding]) -> String {
    let health = HealthCalculator::calculate(index, findings);

    match format {
        ReportFormat::Json => serde_json::to_string_pretty(&json!({
            "repository": index.repo_name,
            "total_files": index.total_files,
            "total_loc": index.total_loc,
            "health": health,
            "findings": findings
        }))
        .unwrap_or_default(),

        ReportFormat::Markdown => {
            let mut out = String::new();
            out.push_str(&format!("# RepoTrek Intelligence Report: {}\n\n", index.repo_name));
            out.push_str(&format!("**Overall Health Score**: {}/100 ({})\n\n", health.overall, health.status));
            out.push_str("## Health Breakdown\n");
            for c in &health.categories {
                out.push_str(&format!("- **{}**: {}/100 ({})\n", c.name, c.score, c.status));
            }
            out.push_str("\n## Discovered Findings\n");
            for f in findings {
                out.push_str(&format!("### [{}] {}\n", f.severity, f.title));
                out.push_str(&format!("*Location*: `{}`\n", f.location));
                out.push_str(&format!("{}\n", f.description));
                out.push_str(&format!("> **Recommendation**: {}\n\n", f.recommendation));
            }
            out
        }

        ReportFormat::Html => {
            format!(
                "<!DOCTYPE html><html><head><title>RepoTrek Report - {}</title><style>body{{font-family:sans-serif;padding:20px;background:#0d1117;color:#c9d1d9;}}h1{{color:#58a6ff;}}.card{{background:#161b22;padding:15px;border-radius:6px;margin-bottom:15px;border:1px solid #30363d;}}</style></head><body><h1>RepoTrek Intelligence Report: {}</h1><div class='card'><h2>Overall Health: {}/100 ({})</h2><p>Files: {} | LOC: {}</p></div></body></html>",
                index.repo_name, index.repo_name, health.overall, health.status, index.total_files, index.total_loc
            )
        }

        ReportFormat::Sarif => serde_json::to_string_pretty(&json!({
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "RepoTrek Intelligence Engine",
                        "version": "0.3.9"
                    }
                },
                "results": findings.iter().map(|f| json!({
                    "ruleId": f.id,
                    "message": { "text": f.title },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": f.location }
                        }
                    }]
                })).collect::<Vec<_>>()
            }]
        }))
        .unwrap_or_default(),
    }
}
