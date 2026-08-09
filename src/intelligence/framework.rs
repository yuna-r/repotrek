use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Low => write!(f, "LOW"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::High => write!(f, "HIGH"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    Heuristic,
    Low,
    Medium,
    High,
    Confirmed,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::Heuristic => write!(f, "HEURISTIC"),
            Confidence::Low => write!(f, "LOW"),
            Confidence::Medium => write!(f, "MEDIUM"),
            Confidence::High => write!(f, "HIGH"),
            Confidence::Confirmed => write!(f, "CONFIRMED"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub file: String,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
    pub snippet: Option<String>,
    pub symbol: Option<String>,
    pub commit: Option<String>,
    pub pr: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub analyzer: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub title: String,
    pub description: String,
    pub evidence: Vec<Evidence>,
    pub location: String,
    pub recommendation: String,
    pub timestamp: DateTime<Utc>,
}

pub trait Analyzer: Send + Sync {
    fn name(&self) -> &'static str;
    #[allow(dead_code)]
    fn version(&self) -> &'static str;
    #[allow(dead_code)]
    fn capabilities(&self) -> Vec<&'static str>;
    fn analyze(&self, index: &crate::intelligence::index::RepoIndex) -> Vec<Finding>;
}
