use crate::intelligence::index::RepoIndex;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    pub file_path: String,
    pub score: f32,
    pub matched_terms: Vec<String>,
    pub snippet: String,
}

#[allow(dead_code)]
pub struct SemanticSearchEngine;

impl SemanticSearchEngine {
    #[allow(dead_code)]
    pub fn search(index: &RepoIndex, query: &str) -> Vec<SemanticSearchResult> {
        let query_terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        let mut results = Vec::new();

        for (path, file) in &index.files {
            let path_lower = path.to_lowercase();
            let content_lower = file.content_preview.to_lowercase();

            let mut matched_terms = Vec::new();
            let mut matches = 0;

            for term in &query_terms {
                if path_lower.contains(term) {
                    matches += 3;
                    matched_terms.push(term.clone());
                } else if content_lower.contains(term) {
                    matches += 1;
                    matched_terms.push(term.clone());
                }
            }

            if matches > 0 {
                let score = (matches as f32 / (query_terms.len() as f32 * 3.0)).min(1.0);
                results.push(SemanticSearchResult {
                    file_path: path.clone(),
                    score,
                    matched_terms,
                    snippet: file.content_preview.clone(),
                });
            }
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}
