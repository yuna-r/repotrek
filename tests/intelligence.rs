use repotrek::intelligence::{
    analyzers::onboarding::OnboardingAnalyzer, format_report, IntelligenceEngine,
    ReportFormat, RepositoryReport,
};
use std::path::Path;

#[test]
fn test_intelligence_local_analysis() {
    let engine = IntelligenceEngine::new();
    let root = Path::new(".");
    let result = engine.analyze_local(root);

    assert!(result.is_ok(), "Local directory analysis should succeed");

    let (index, findings, health) = result.unwrap();
    assert!(index.total_files > 0, "RepoIndex should find workspace files");
    assert!(health.overall <= 100, "Health score must be between 0 and 100");

    let report_json = format_report(ReportFormat::Json, &index, &findings);
    assert!(report_json.contains("overall"), "JSON report should contain health score");

    let guide = OnboardingAnalyzer::generate_guide(&index);
    assert!(!guide.recommended_reading_order.is_empty(), "Onboarding guide should contain reading steps");

    let summary = RepositoryReport::generate(&index, &findings);
    assert!(summary.contains("REPOTREK INTELLIGENCE EXECUTIVE REPORT"), "Executive report should render correctly");
}
