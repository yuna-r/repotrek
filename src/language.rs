use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedLanguage {
    Rust,
    C,
    Cpp,
    CSharp,
    Java,
    Kotlin,
    Swift,
    Go,
    Python,
    Ruby,
    Shell,
    JavaScript,
    TypeScript,
    Json,
    Yaml,
    Toml,
    Sql,
    Html,
    Css,
    Markdown,
    Lua,
    Haskell,
    Make,
    Dockerfile,
    CMake,
    Starlark,
    Groovy,
    Perl,
    Php,
    Other,
}

impl DetectedLanguage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Java => "Java",
            Self::Kotlin => "Kotlin",
            Self::Swift => "Swift",
            Self::Go => "Go",
            Self::Python => "Python",
            Self::Ruby => "Ruby",
            Self::Shell => "Shell",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::Sql => "SQL",
            Self::Html => "HTML/XML",
            Self::Css => "CSS",
            Self::Markdown => "Markdown",
            Self::Lua => "Lua",
            Self::Haskell => "Haskell",
            Self::Make => "Make",
            Self::Dockerfile => "Dockerfile",
            Self::CMake => "CMake",
            Self::Starlark => "Starlark",
            Self::Groovy => "Groovy",
            Self::Perl => "Perl",
            Self::Php => "PHP",
            Self::Other => "Unknown",
        }
    }
}

#[must_use]
pub fn detect_language(path: &str, content: &str) -> DetectedLanguage {
    let path = Path::new(path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let lower_name = file_name.to_ascii_lowercase();

    if let Some(language) = language_for_file_name(&lower_name) {
        return language;
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "h" {
        return if looks_like_cpp_header(content) {
            DetectedLanguage::Cpp
        } else {
            DetectedLanguage::C
        };
    }
    if let Some(language) = language_for_extension(&extension) {
        return language;
    }

    if let Some(language) = language_for_shebang(content) {
        return language;
    }

    language_from_content(content)
}

fn language_for_file_name(file_name: &str) -> Option<DetectedLanguage> {
    let language = match file_name {
        "makefile" | "gnumakefile" | "bsdmakefile" | "justfile" => DetectedLanguage::Make,
        "cmakelists.txt" => DetectedLanguage::CMake,
        "gemfile" | "rakefile" | "vagrantfile" | "guardfile" | "podfile" | "fastfile"
        | "brewfile" | "capfile" | "berksfile" | "puppetfile" | "thorfile" => {
            DetectedLanguage::Ruby
        }
        "sconstruct" | "sconscript" | "wscript" => DetectedLanguage::Python,
        "configure" | "gradlew" | "mvnw" => DetectedLanguage::Shell,
        "jenkinsfile" => DetectedLanguage::Groovy,
        "build" | "build.bazel" | "workspace" | "workspace.bazel" | "buck" => {
            DetectedLanguage::Starlark
        }
        "procfile" => DetectedLanguage::Shell,
        "cargo.lock" => DetectedLanguage::Toml,
        _ if file_name.starts_with("makefile.") => DetectedLanguage::Make,
        _ if file_name == "dockerfile" || file_name.starts_with("dockerfile.") => {
            DetectedLanguage::Dockerfile
        }
        _ => return None,
    };
    Some(language)
}

fn language_for_extension(extension: &str) -> Option<DetectedLanguage> {
    let language = match extension {
        "rs" => DetectedLanguage::Rust,
        "c" => DetectedLanguage::C,
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => DetectedLanguage::Cpp,
        "cs" => DetectedLanguage::CSharp,
        "java" => DetectedLanguage::Java,
        "kt" | "kts" => DetectedLanguage::Kotlin,
        "swift" => DetectedLanguage::Swift,
        "go" => DetectedLanguage::Go,
        "py" | "pyi" | "pyw" => DetectedLanguage::Python,
        "rb" => DetectedLanguage::Ruby,
        "sh" | "bash" | "zsh" | "fish" => DetectedLanguage::Shell,
        "js" | "jsx" | "mjs" | "cjs" => DetectedLanguage::JavaScript,
        "ts" | "tsx" | "mts" | "cts" => DetectedLanguage::TypeScript,
        "json" | "jsonc" => DetectedLanguage::Json,
        "yaml" | "yml" => DetectedLanguage::Yaml,
        "toml" => DetectedLanguage::Toml,
        "sql" => DetectedLanguage::Sql,
        "html" | "htm" | "xml" | "svg" => DetectedLanguage::Html,
        "css" | "scss" | "sass" | "less" => DetectedLanguage::Css,
        "md" | "markdown" | "mdx" => DetectedLanguage::Markdown,
        "lua" => DetectedLanguage::Lua,
        "hs" | "lhs" => DetectedLanguage::Haskell,
        "mk" | "mak" => DetectedLanguage::Make,
        "cmake" => DetectedLanguage::CMake,
        "bzl" | "bazel" => DetectedLanguage::Starlark,
        "groovy" | "gradle" => DetectedLanguage::Groovy,
        "pl" | "pm" => DetectedLanguage::Perl,
        "php" | "php3" | "php4" | "php5" | "phtml" => DetectedLanguage::Php,
        _ => return None,
    };
    Some(language)
}

fn language_for_shebang(content: &str) -> Option<DetectedLanguage> {
    let first_line = content.lines().next()?.trim();
    if !first_line.starts_with("#!") {
        return None;
    }
    let lower = first_line.to_ascii_lowercase();
    let language = if lower.contains("python") {
        DetectedLanguage::Python
    } else if lower.contains("ruby") {
        DetectedLanguage::Ruby
    } else if lower.contains("node") || lower.contains("deno") || lower.contains("bun") {
        DetectedLanguage::JavaScript
    } else if lower.contains("perl") {
        DetectedLanguage::Perl
    } else if lower.contains("php") {
        DetectedLanguage::Php
    } else if lower.contains("groovy") {
        DetectedLanguage::Groovy
    } else if lower.contains("bash")
        || lower.contains("zsh")
        || lower.contains("fish")
        || lower.ends_with("/sh")
        || lower.contains(" env sh")
    {
        DetectedLanguage::Shell
    } else {
        return None;
    };
    Some(language)
}

fn language_from_content(content: &str) -> DetectedLanguage {
    let sample = content.lines().take(80).collect::<Vec<_>>();
    let trimmed = content.trim_start();

    if sample.iter().any(|line| {
        let line = line.trim_start();
        line.starts_with("cmake_minimum_required(") || line.starts_with("project(")
    }) {
        return DetectedLanguage::CMake;
    }
    if sample
        .iter()
        .any(|line| line.trim_start().starts_with("FROM "))
    {
        return DetectedLanguage::Dockerfile;
    }
    if looks_like_makefile(&sample) {
        return DetectedLanguage::Make;
    }
    if sample.iter().any(|line| {
        let line = line.trim_start();
        line.starts_with("use crate::") || line.starts_with("extern crate ")
    }) {
        return DetectedLanguage::Rust;
    }
    if sample.iter().any(|line| {
        let line = line.trim_start();
        (line.starts_with("def ") || line.starts_with("class ")) && line.ends_with(':')
    }) {
        return DetectedLanguage::Python;
    }
    if sample.iter().any(|line| {
        let line = line.trim_start();
        line.starts_with("package main") || line.starts_with("func main(")
    }) {
        return DetectedLanguage::Go;
    }
    if trimmed.starts_with("<?php") {
        return DetectedLanguage::Php;
    }
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && sample
            .iter()
            .any(|line| line.contains("\":") || line.contains("\" :"))
    {
        return DetectedLanguage::Json;
    }

    DetectedLanguage::Other
}

fn looks_like_cpp_header(content: &str) -> bool {
    content.lines().take(120).any(|line| {
        let line = line.trim_start();
        line.starts_with("namespace ")
            || line.starts_with("template<")
            || line.starts_with("template <")
            || line.contains("std::")
            || line.contains("constexpr")
            || line.contains("noexcept")
            || line.contains("public:")
            || line.contains("private:")
            || line.contains("protected:")
    })
}

fn looks_like_makefile(lines: &[&str]) -> bool {
    let has_recipe = lines.iter().any(|line| line.starts_with('\t'));
    let has_target = lines.iter().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || (line.starts_with('.') && line.contains('='))
        {
            return false;
        }
        line.split_once(':').is_some_and(|(target, rest)| {
            !target.contains('=')
                && !target.contains("//")
                && !target.chars().any(char::is_whitespace)
                && !rest.starts_with('=')
        })
    });
    let has_make_variable = lines.iter().any(|line| {
        let line = line.trim();
        line.contains(":=") || line.contains("?=") || line.contains("+=")
    });
    has_target && (has_recipe || has_make_variable)
}

#[cfg(test)]
mod tests {
    use super::{DetectedLanguage, detect_language};

    #[test]
    fn detects_extensionless_build_files() {
        assert_eq!(
            detect_language("Makefile", "all:\n\tcargo build\n"),
            DetectedLanguage::Make
        );
        assert_eq!(
            detect_language("Dockerfile.dev", "FROM rust:latest\n"),
            DetectedLanguage::Dockerfile
        );
        assert_eq!(
            detect_language("CMakeLists.txt", "project(example)\n"),
            DetectedLanguage::CMake
        );
        assert_eq!(
            detect_language("Makefile.in", "all:\n\t@echo ok\n"),
            DetectedLanguage::Make
        );
        assert_eq!(
            detect_language("SConstruct", "env = Environment()\n"),
            DetectedLanguage::Python
        );
    }

    #[test]
    fn detects_extensionless_scripts_by_shebang() {
        assert_eq!(
            detect_language("tools/release", "#!/usr/bin/env python3\nprint('ok')\n"),
            DetectedLanguage::Python
        );
    }
}
