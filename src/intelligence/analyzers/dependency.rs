use crate::intelligence::framework::{Analyzer, Confidence, Evidence, Finding, Severity};
use crate::intelligence::index::RepoIndex;
use chrono::Utc;
use std::collections::{HashMap, HashSet};

pub struct DependencyAnalyzer;

impl Analyzer for DependencyAnalyzer {
    fn name(&self) -> &'static str {
        "DependencyAnalyzer"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn capabilities(&self) -> Vec<&'static str> {
        vec!["dependency_graph", "circular_dependency_detection"]
    }

    fn analyze(&self, index: &RepoIndex) -> Vec<Finding> {
        let mut findings = Vec::new();

        let mut mod_deps: HashMap<String, HashSet<String>> = HashMap::new();
        for (path, file) in &index.files {
            let mut set = HashSet::new();
            for import in &file.imports {
                for (other_path, _) in &index.files {
                    if path != other_path {
                        let stem = other_path
                            .trim_start_matches("src/")
                            .trim_end_matches(".rs")
                            .replace('/', "::");
                        if import.contains(&stem) {
                            set.insert(other_path.clone());
                        }
                    }
                }
            }
            mod_deps.insert(path.clone(), set);
        }

        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut on_stack = HashSet::new();
        let mut cycles = Vec::new();

        fn dfs(
            node: &str,
            graph: &HashMap<String, HashSet<String>>,
            visited: &mut HashSet<String>,
            stack: &mut Vec<String>,
            on_stack: &mut HashSet<String>,
            cycles: &mut Vec<Vec<String>>,
        ) {
            visited.insert(node.to_string());
            on_stack.insert(node.to_string());
            stack.push(node.to_string());

            if let Some(neighbors) = graph.get(node) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        dfs(neighbor, graph, visited, stack, on_stack, cycles);
                    } else if on_stack.contains(neighbor) {
                        if let Some(pos) = stack.iter().position(|x| x == neighbor) {
                            let mut cycle = stack[pos..].to_vec();
                            cycle.push(neighbor.clone());
                            cycles.push(cycle);
                        }
                    }
                }
            }

            stack.pop();
            on_stack.remove(node);
        }

        for node in mod_deps.keys() {
            if !visited.contains(node) {
                dfs(node, &mod_deps, &mut visited, &mut stack, &mut on_stack, &mut cycles);
            }
        }

        for cycle in cycles {
            let cycle_str = cycle.join(" -> ");
            findings.push(Finding {
                id: "DEP-001".into(),
                analyzer: self.name().into(),
                severity: Severity::High,
                confidence: Confidence::Confirmed,
                title: "Circular Module Dependency Detected".into(),
                description: format!("Circular dependency cycle found: {}", cycle_str),
                evidence: vec![Evidence {
                    file: cycle[0].clone(),
                    line_start: Some(1),
                    line_end: None,
                    snippet: None,
                    symbol: None,
                    commit: None,
                    pr: None,
                }],
                location: cycle[0].clone(),
                recommendation: "Refactor shared logic into a common module to break the cycle.".into(),
                timestamp: Utc::now(),
            });
        }

        if !index.dependencies.is_empty() {
            findings.push(Finding {
                id: "DEP-002".into(),
                analyzer: self.name().into(),
                severity: Severity::Info,
                confidence: Confidence::Confirmed,
                title: format!("Direct External Dependencies: {}", index.dependencies.len()),
                description: format!("Repository relies on {} external packages.", index.dependencies.len()),
                evidence: vec![],
                location: "Cargo.toml / Package Manifest".into(),
                recommendation: "Audit external packages periodically for maintenance and updates.".into(),
                timestamp: Utc::now(),
            });
        }

        findings
    }
}
