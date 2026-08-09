use crate::intelligence::index::RepoIndex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingGuide {
    pub repo_name: String,
    pub description: String,
    pub entry_points: Vec<String>,
    pub key_modules: Vec<String>,
    pub recommended_reading_order: Vec<String>,
    pub build_system: String,
}

pub struct OnboardingAnalyzer;

impl OnboardingAnalyzer {
    pub fn generate_guide(index: &RepoIndex) -> OnboardingGuide {
        let mut reading_order = Vec::new();

        if index.files.keys().any(|p| p.to_lowercase().starts_with("readme")) {
            reading_order.push("1. README.md (Overview & Quickstart)".to_string());
        }
        if index.files.contains_key("Cargo.toml") {
            reading_order.push("2. Cargo.toml (Project Manifest & Dependencies)".to_string());
        }
        if index.files.contains_key("src/main.rs") {
            reading_order.push("3. src/main.rs (CLI Entrypoint & Command Dispatcher)".to_string());
        } else if index.files.contains_key("src/lib.rs") {
            reading_order.push("3. src/lib.rs (Library Root & Exported Modules)".to_string());
        }
        reading_order.push("4. src/app.rs (Core Application State)".to_string());
        reading_order.push("5. src/ui.rs (Terminal User Interface Renderer)".to_string());
        reading_order.push("6. tests/ (Unit and Integration Test Suite)".to_string());

        let key_modules = index
            .files
            .keys()
            .filter(|p| p.starts_with("src/") && p.ends_with(".rs"))
            .take(6)
            .cloned()
            .collect();

        OnboardingGuide {
            repo_name: index.repo_name.clone(),
            description: "Repository architecture and developer onboarding guide.".into(),
            entry_points: vec!["src/main.rs".into()],
            key_modules,
            recommended_reading_order: reading_order,
            build_system: "Cargo / Rust standard toolchain".into(),
        }
    }
}
