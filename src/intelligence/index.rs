use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    pub path: String,
    pub extension: String,
    pub language: String,
    pub size_bytes: u64,
    pub line_count: usize,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub symbols: Vec<String>,
    pub content_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedCommit {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub files_changed: Vec<String>,
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoIndex {
    pub repo_name: String,
    pub default_branch: String,
    pub total_files: usize,
    pub total_loc: usize,
    pub files: HashMap<String, IndexedFile>,
    pub dependencies: HashMap<String, String>,
    pub commit_history: Vec<IndexedCommit>,
    pub workflows: Vec<String>,
}

impl RepoIndex {
    pub fn new(repo_name: impl Into<String>) -> Self {
        Self {
            repo_name: repo_name.into(),
            default_branch: "main".into(),
            total_files: 0,
            total_loc: 0,
            files: HashMap::new(),
            dependencies: HashMap::new(),
            commit_history: Vec::new(),
            workflows: Vec::new(),
        }
    }

    pub fn scan_local_directory(&mut self, root_path: &Path) -> anyhow::Result<()> {
        let mut files = HashMap::new();
        let mut total_loc = 0;

        fn walk_dir(
            dir: &Path,
            root: &Path,
            files: &mut HashMap<String, IndexedFile>,
            total_loc: &mut usize,
        ) -> std::io::Result<()> {
            if dir.is_dir() {
                for entry in std::fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                    if file_name.starts_with('.')
                        || file_name == "target"
                        || file_name == "node_modules"
                        || file_name == "vendor"
                    {
                        continue;
                    }

                    if path.is_dir() {
                        walk_dir(&path, root, files, total_loc)?;
                    } else if path.is_file() {
                        let rel_path = path
                            .strip_prefix(root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        let ext = path
                            .extension()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase();
                        let metadata = std::fs::metadata(&path)?;

                        let content = std::fs::read_to_string(&path).unwrap_or_default();
                        let line_count = content.lines().count();
                        *total_loc += line_count;

                        let lang = match ext.as_str() {
                            "rs" => "Rust",
                            "ts" | "tsx" => "TypeScript",
                            "js" | "jsx" => "JavaScript",
                            "py" => "Python",
                            "go" => "Go",
                            "java" => "Java",
                            "c" | "h" => "C",
                            "cpp" | "hpp" => "C++",
                            "cs" => "C#",
                            "json" => "JSON",
                            "toml" => "TOML",
                            "yaml" | "yml" => "YAML",
                            "md" => "Markdown",
                            _ => "Text",
                        }
                        .to_string();

                        let preview = content.lines().take(10).collect::<Vec<_>>().join("\n");
                        let imports = parse_imports(&content, &ext);

                        files.insert(
                            rel_path.clone(),
                            IndexedFile {
                                path: rel_path,
                                extension: ext,
                                language: lang,
                                size_bytes: metadata.len(),
                                line_count,
                                imports,
                                exports: Vec::new(),
                                symbols: Vec::new(),
                                content_preview: preview,
                            },
                        );
                    }
                }
            }
            Ok(())
        }

        walk_dir(root_path, root_path, &mut files, &mut total_loc)?;

        self.total_files = files.len();
        self.total_loc = total_loc;
        self.files = files;

        let cargo_toml = root_path.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                let mut in_deps = false;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("[dependencies]") {
                        in_deps = true;
                        continue;
                    }
                    if trimmed.starts_with('[') {
                        in_deps = false;
                    }
                    if in_deps && trimmed.contains('=') {
                        let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
                        let name = parts[0].trim().to_string();
                        let ver = parts[1].trim().trim_matches('"').trim_matches('\'').to_string();
                        self.dependencies.insert(name, ver);
                    }
                }
            }
        }

        Ok(())
    }
}

fn parse_imports(content: &str, ext: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if ext == "rs" && (trimmed.starts_with("use ") || trimmed.starts_with("mod ")) {
            imports.push(trimmed.to_string());
        } else if (ext == "ts" || ext == "js") && (trimmed.starts_with("import ") || trimmed.starts_with("require")) {
            imports.push(trimmed.to_string());
        } else if ext == "py" && (trimmed.starts_with("import ") || trimmed.starts_with("from ")) {
            imports.push(trimmed.to_string());
        } else if ext == "go" && trimmed.starts_with("import") {
            imports.push(trimmed.to_string());
        }
    }
    imports
}
