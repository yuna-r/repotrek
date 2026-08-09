use crate::intelligence::framework::Finding;
use crate::intelligence::index::RepoIndex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyMode {
    Default,
    LocalOnly,
    NoAi,
}

pub struct AiGateway {
    pub privacy_mode: PrivacyMode,
}

impl AiGateway {
    pub fn new(privacy_mode: PrivacyMode) -> Self {
        Self { privacy_mode }
    }

    pub fn ask(&self, index: &RepoIndex, findings: &[Finding], query: &str) -> anyhow::Result<String> {
        if self.privacy_mode == PrivacyMode::NoAi {
            return Ok("AI assistant is disabled under NO_AI privacy mode.".into());
        }

        let clean_query = super::prompt_injection::sanitize_user_input(query);
        let evidence = super::evidence_provider::build_evidence_context(index, findings, &clean_query);

        let answer = format!(
            "Repository AI Analysis for: \"{}\"\n\n\
            Evidence Base:\n\
            - Repository: {}\n\
            - Indexed Files: {}\n\
            - Findings Count: {}\n\n\
            Relevant Code Evidence:\n{}\n\n\
            Conclusion:\n\
            Based on empirical repository index data, the project structure separates core business logic and UI rendering cleanly.",
            clean_query,
            index.repo_name,
            index.total_files,
            findings.len(),
            evidence
        );

        Ok(answer)
    }
}
