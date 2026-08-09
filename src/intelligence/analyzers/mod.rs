pub mod api;
pub mod architecture;
pub mod ci;
pub mod dependency;
pub mod diff;
pub mod documentation;
pub mod history;
pub mod hotspot;
pub mod onboarding;
pub mod ownership;
pub mod quality;
pub mod risk;
pub mod security;
pub mod semantic;
pub mod vulnerability;

use crate::intelligence::framework::{Analyzer, Finding};
use crate::intelligence::index::RepoIndex;

pub fn run_all_analyzers(index: &RepoIndex) -> Vec<Finding> {
    let analyzers: Vec<Box<dyn Analyzer>> = vec![
        Box::new(architecture::ArchitectureAnalyzer),
        Box::new(dependency::DependencyAnalyzer),
        Box::new(security::SecurityAnalyzer),
        Box::new(vulnerability::VulnerabilityAnalyzer),
        Box::new(quality::QualityAnalyzer),
        Box::new(hotspot::HotspotAnalyzer),
        Box::new(history::HistoryAnalyzer),
        Box::new(ownership::OwnershipAnalyzer),
        Box::new(risk::RiskAnalyzer),
        Box::new(ci::CiAnalyzer),
        Box::new(documentation::DocumentationAnalyzer),
        Box::new(api::ApiAnalyzer),
    ];

    let mut findings = Vec::new();
    for a in analyzers {
        findings.extend(a.analyze(index));
    }
    findings
}
