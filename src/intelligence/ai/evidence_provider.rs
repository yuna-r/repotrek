use crate::intelligence::framework::Finding;
use crate::intelligence::index::RepoIndex;

pub fn build_evidence_context(index: &RepoIndex, findings: &[Finding], query: &str) -> String {
    let mut lines = Vec::new();
    let query_lower = query.to_lowercase();

    for (path, file) in &index.files {
        if path.to_lowercase().contains(&query_lower) {
            lines.push(format!("- File: {} ({} LOC)", path, file.line_count));
        }
    }

    for f in findings {
        if f.title.to_lowercase().contains(&query_lower) || f.description.to_lowercase().contains(&query_lower) {
            lines.push(format!("- Finding [{}] {}: {}", f.severity, f.title, f.location));
        }
    }

    if lines.is_empty() {
        lines.push(format!("- Index baseline: {} files across repository.", index.total_files));
    }

    lines.join("\n")
}
