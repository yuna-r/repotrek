use crate::intelligence::framework::Finding;
use crate::intelligence::health::HealthCalculator;
use crate::intelligence::index::RepoIndex;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

pub struct McpServer {
    index: RepoIndex,
    findings: Vec<Finding>,
}

impl McpServer {
    pub fn new(index: RepoIndex, findings: Vec<Finding>) -> Self {
        Self { index, findings }
    }

    pub fn run_stdio(&self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(req) = serde_json::from_str::<Value>(&line) {
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

                let response = match method {
                    "tools/list" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "tools": [
                                { "name": "get_repository", "description": "Get repository general info and metrics" },
                                { "name": "get_architecture", "description": "Get entry points and layer architecture" },
                                { "name": "get_security", "description": "Get security findings and secret exposure warnings" },
                                { "name": "get_quality", "description": "Get code quality and complexity metrics" },
                                { "name": "get_dependencies", "description": "Get external dependencies and vulnerability findings" },
                                { "name": "get_health", "description": "Get composite Repo Health Score (0-100)" },
                                { "name": "search_code", "description": "Perform semantic or exact code search" }
                            ]
                        }
                    }),
                    "tools/call" => {
                        let params = req.get("params");
                        let tool_name = params.and_then(|p| p.get("name")).and_then(|n| n.as_str()).unwrap_or("");
                        let result_data = self.handle_tool_call(tool_name);

                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&result_data).unwrap_or_default()
                                }]
                            }
                        })
                    }
                    _ => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "Method not found" }
                    }),
                };

                writeln!(stdout, "{}", response)?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    fn handle_tool_call(&self, tool_name: &str) -> Value {
        match tool_name {
            "get_repository" => json!({
                "name": self.index.repo_name,
                "total_files": self.index.total_files,
                "total_loc": self.index.total_loc,
            }),
            "get_health" => {
                let health = HealthCalculator::calculate(&self.index, &self.findings);
                json!(health)
            }
            "get_security" => {
                let sec_findings: Vec<_> = self.findings.iter().filter(|f| f.analyzer == "SecurityAnalyzer" || f.analyzer == "VulnerabilityAnalyzer").collect();
                json!(sec_findings)
            }
            "get_architecture" => {
                let arch_findings: Vec<_> = self.findings.iter().filter(|f| f.analyzer == "ArchitectureAnalyzer").collect();
                json!(arch_findings)
            }
            "get_quality" => {
                let qual_findings: Vec<_> = self.findings.iter().filter(|f| f.analyzer == "QualityAnalyzer").collect();
                json!(qual_findings)
            }
            "get_dependencies" => json!({
                "dependencies": self.index.dependencies,
            }),
            _ => json!({ "error": "Unknown tool" }),
        }
    }
}
