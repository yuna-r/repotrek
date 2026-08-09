use crate::intelligence::framework::Finding;
use crate::intelligence::health::HealthCalculator;
use crate::intelligence::index::RepoIndex;

pub struct RepositoryReport;

impl RepositoryReport {
    pub fn generate(index: &RepoIndex, findings: &[Finding]) -> String {
        let health = HealthCalculator::calculate(index, findings);

        format!(
            "====================================================\n\
            REPOTREK INTELLIGENCE EXECUTIVE REPORT\n\
            ====================================================\n\
            Repository: {}\n\
            Files Analyzed: {}\n\
            Total Lines of Code: {}\n\
            Overall Health Score: {}/100 ({})\n\n\
            HEALTH BREAKDOWN:\n\
            - Security: {}/100\n\
            - Architecture: {}/100\n\
            - Dependencies: {}/100\n\
            - Quality: {}/100\n\
            - Documentation: {}/100\n\n\
            EXECUTIVE FINDINGS SUMMARY:\n\
            - Total Findings: {}\n\
            - Critical Severity: {}\n\
            - High Severity: {}\n\n\
            RECOMMENDATIONS:\n\
            1. Review critical security findings and eliminate secret exposures.\n\
            2. Maintain modular separation between API delivery and core domains.\n\
            3. Expand automated unit testing coverage.\n\
            ====================================================\n",
            index.repo_name,
            index.total_files,
            index.total_loc,
            health.overall,
            health.status,
            health.categories.iter().find(|c| c.name == "Security").map(|c| c.score).unwrap_or(0),
            health.categories.iter().find(|c| c.name == "Architecture").map(|c| c.score).unwrap_or(0),
            health.categories.iter().find(|c| c.name == "Dependencies").map(|c| c.score).unwrap_or(0),
            health.categories.iter().find(|c| c.name == "Quality").map(|c| c.score).unwrap_or(0),
            health.categories.iter().find(|c| c.name == "Documentation").map(|c| c.score).unwrap_or(0),
            health.total_findings,
            health.critical_count,
            health.high_count,
        )
    }
}
