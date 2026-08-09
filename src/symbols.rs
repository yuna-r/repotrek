use crate::{
    language::{DetectedLanguage, detect_language},
    model::SymbolLocation,
};

#[must_use]
pub fn extract_symbols(path: &str, content: &str) -> Vec<SymbolLocation> {
    let language = detect_language(path, content);
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| symbol_from_line(language, line, index + 1))
        .collect()
}

fn symbol_from_line(
    language: DetectedLanguage,
    line: &str,
    line_number: usize,
) -> Option<SymbolLocation> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    match language {
        DetectedLanguage::Rust => rust_symbol(trimmed, line_number),
        DetectedLanguage::Python => prefixed_symbol(
            trimmed,
            &[("async def ", "fn"), ("def ", "fn"), ("class ", "class")],
            line_number,
        ),
        DetectedLanguage::Go => go_symbol(trimmed, line_number),
        DetectedLanguage::JavaScript | DetectedLanguage::TypeScript => {
            js_symbol(trimmed, line_number)
        }
        DetectedLanguage::Java
        | DetectedLanguage::Kotlin
        | DetectedLanguage::CSharp
        | DetectedLanguage::Swift => managed_symbol(language, trimmed, line_number),
        DetectedLanguage::C | DetectedLanguage::Cpp => c_family_symbol(trimmed, line_number),
        DetectedLanguage::Ruby => prefixed_symbol(
            trimmed,
            &[("class ", "class"), ("module ", "module"), ("def ", "fn")],
            line_number,
        ),
        DetectedLanguage::Php => prefixed_symbol(
            trimmed,
            &[
                ("function ", "fn"),
                ("class ", "class"),
                ("interface ", "interface"),
                ("trait ", "trait"),
                ("enum ", "enum"),
            ],
            line_number,
        ),
        DetectedLanguage::Shell => shell_symbol(trimmed, line_number),
        DetectedLanguage::Make => make_symbol(line, line_number),
        DetectedLanguage::CMake => cmake_symbol(trimmed, line_number),
        DetectedLanguage::Dockerfile => docker_symbol(trimmed, line_number),
        DetectedLanguage::Groovy => managed_symbol(language, trimmed, line_number),
        DetectedLanguage::Starlark => prefixed_symbol(trimmed, &[("def ", "fn")], line_number),
        DetectedLanguage::Lua => prefixed_symbol(
            trimmed,
            &[("local function ", "fn"), ("function ", "fn")],
            line_number,
        ),
        DetectedLanguage::Perl => prefixed_symbol(
            trimmed,
            &[("sub ", "fn"), ("package ", "module")],
            line_number,
        ),
        DetectedLanguage::Haskell => haskell_symbol(trimmed, line_number),
        DetectedLanguage::Sql => sql_symbol(trimmed, line_number),
        DetectedLanguage::Toml => section_symbol(trimmed, '[', ']', "section", line_number),
        DetectedLanguage::Yaml => yaml_symbol(line, line_number),
        DetectedLanguage::Markdown => markdown_symbol(trimmed, line_number),
        DetectedLanguage::Json
        | DetectedLanguage::Html
        | DetectedLanguage::Css
        | DetectedLanguage::Other => None,
    }
}

fn location(name: &str, kind: &str, line: usize) -> Option<SymbolLocation> {
    let name = name.trim_matches(|character: char| {
        matches!(
            character,
            '(' | ')' | '{' | '}' | '[' | ']' | ':' | ',' | ';' | '\'' | '"'
        )
    });
    if name.is_empty() {
        None
    } else {
        Some(SymbolLocation {
            name: name.to_owned(),
            kind: kind.to_owned(),
            line,
        })
    }
}

fn prefixed_symbol(
    trimmed: &str,
    candidates: &[(&str, &str)],
    line_number: usize,
) -> Option<SymbolLocation> {
    if trimmed.starts_with('#') || trimmed.starts_with('/') {
        return None;
    }
    for (prefix, kind) in candidates {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name = identifier_prefix(rest);
            return location(name, kind, line_number);
        }
    }
    None
}

fn rust_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    if trimmed.starts_with("//") || trimmed.starts_with('#') {
        return None;
    }
    let mut value = trimmed;
    if let Some(rest) = value.strip_prefix("pub ") {
        value = rest;
    } else if value.starts_with("pub(")
        && let Some((_, rest)) = value.split_once(") ")
    {
        value = rest;
    }
    if let Some(rest) = value.strip_prefix("unsafe ") {
        value = rest;
    }
    if let Some(rest) = value.strip_prefix("async ") {
        value = rest;
    }
    prefixed_symbol(
        value,
        &[
            ("fn ", "fn"),
            ("struct ", "struct"),
            ("enum ", "enum"),
            ("trait ", "trait"),
            ("union ", "union"),
            ("type ", "type"),
            ("const ", "const"),
            ("static ", "static"),
            ("mod ", "mod"),
            ("impl ", "impl"),
            ("macro_rules! ", "macro"),
        ],
        line_number,
    )
}

fn go_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    if let Some(rest) = trimmed.strip_prefix("func ") {
        let rest = if rest.starts_with('(') {
            rest.split_once(')')
                .map_or(rest, |(_, after)| after.trim_start())
        } else {
            rest
        };
        return location(identifier_prefix(rest), "fn", line_number);
    }
    prefixed_symbol(
        trimmed,
        &[("type ", "type"), ("const ", "const"), ("var ", "var")],
        line_number,
    )
}

fn js_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    if trimmed.starts_with("//") {
        return None;
    }
    let mut value = trimmed;
    for prefix in ["export default ", "export ", "declare ", "async "] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest;
        }
    }
    if let Some(symbol) = prefixed_symbol(
        value,
        &[
            ("async function ", "fn"),
            ("function ", "fn"),
            ("class ", "class"),
            ("interface ", "interface"),
            ("type ", "type"),
            ("enum ", "enum"),
            ("namespace ", "namespace"),
        ],
        line_number,
    ) {
        return Some(symbol);
    }
    for prefix in ["const ", "let ", "var "] {
        if let Some(rest) = value.strip_prefix(prefix)
            && let Some((name, rhs)) = rest.split_once('=')
            && (rhs.contains("=>") || rhs.trim_start().starts_with("function"))
        {
            return location(identifier_prefix(name.trim()), "fn", line_number);
        }
    }
    None
}

fn managed_symbol(
    language: DetectedLanguage,
    trimmed: &str,
    line_number: usize,
) -> Option<SymbolLocation> {
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with('#') {
        return None;
    }
    let mut value = trimmed;
    loop {
        let mut changed = false;
        for prefix in [
            "public ",
            "private ",
            "protected ",
            "internal ",
            "static ",
            "final ",
            "abstract ",
            "sealed ",
            "open ",
            "data ",
            "async ",
            "override ",
            "suspend ",
        ] {
            if let Some(rest) = value.strip_prefix(prefix) {
                value = rest;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut candidates = vec![
        ("class ", "class"),
        ("interface ", "interface"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("record ", "record"),
        ("protocol ", "protocol"),
        ("trait ", "trait"),
    ];
    if language == DetectedLanguage::Kotlin {
        candidates.push(("fun ", "fn"));
        candidates.push(("object ", "object"));
    }
    if language == DetectedLanguage::Swift {
        candidates.push(("func ", "fn"));
        candidates.push(("actor ", "actor"));
        candidates.push(("extension ", "extension"));
    }
    if language == DetectedLanguage::Groovy {
        candidates.push(("def ", "fn"));
    }
    if let Some(symbol) = prefixed_symbol(value, &candidates, line_number) {
        return Some(symbol);
    }

    if value.contains('(')
        && (value.ends_with('{') || value.ends_with("=>") || value.ends_with(';'))
        && !starts_control_statement(value)
    {
        let before_paren = value.split('(').next()?.trim_end();
        let name = before_paren.split_whitespace().last().unwrap_or_default();
        if is_identifier(name) {
            return location(name, "fn", line_number);
        }
    }
    None
}

fn c_family_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return None;
    }
    if let Some(symbol) = prefixed_symbol(
        trimmed,
        &[
            ("class ", "class"),
            ("struct ", "struct"),
            ("enum ", "enum"),
            ("union ", "union"),
            ("namespace ", "namespace"),
        ],
        line_number,
    ) {
        return Some(symbol);
    }
    if let Some(rest) = trimmed.strip_prefix("#define ") {
        return location(identifier_prefix(rest), "macro", line_number);
    }
    if trimmed.contains('(')
        && (trimmed.ends_with('{') || trimmed.ends_with(')') || trimmed.ends_with(';'))
        && !starts_control_statement(trimmed)
    {
        let before_paren = trimmed.split('(').next()?.trim_end();
        let name = before_paren
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .trim_matches(|character: char| matches!(character, '*' | '&' | ':' | '~'));
        if is_identifier(name) {
            return location(name, "fn", line_number);
        }
    }
    None
}

fn shell_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    if trimmed.starts_with('#') {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("function ") {
        return location(identifier_prefix(rest), "fn", line_number);
    }
    if let Some(before) = trimmed
        .strip_suffix("() {")
        .or_else(|| trimmed.strip_suffix("(){"))
    {
        return location(before.trim(), "fn", line_number);
    }
    None
}

fn is_make_target(line: &str) -> bool {
    let trimmed = line.trim_end();
    if line.starts_with('\t')
        || line.starts_with(' ')
        || trimmed.is_empty()
        || trimmed.starts_with('#')
    {
        return false;
    }
    let Some((left, _)) = trimmed.split_once(':') else {
        return false;
    };
    !left.contains('=') && !left.is_empty() && !left.starts_with('.')
}

fn make_symbol(line: &str, line_number: usize) -> Option<SymbolLocation> {
    if is_make_target(line) {
        let name = line.split_once(':')?.0.split_whitespace().next()?;
        return location(name, "target", line_number);
    }

    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || line.starts_with('\t') {
        return None;
    }
    for operator in [":=", "?=", "+=", "!=", "="] {
        if let Some((name, _)) = trimmed.split_once(operator) {
            let name = name.trim();
            if !name.is_empty()
                && name.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
            {
                return location(name, "variable", line_number);
            }
        }
    }
    None
}

fn cmake_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    if trimmed.starts_with('#') {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    for (prefix, kind) in [
        ("function(", "fn"),
        ("macro(", "macro"),
        ("project(", "project"),
        ("add_executable(", "target"),
        ("add_library(", "target"),
    ] {
        if lower.starts_with(prefix) {
            let rest = &trimmed[prefix.len()..];
            return location(identifier_prefix(rest.trim_start()), kind, line_number);
        }
    }
    None
}

fn docker_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    let words = trimmed.split_whitespace().collect::<Vec<_>>();
    if words
        .first()
        .is_some_and(|word| word.eq_ignore_ascii_case("from"))
    {
        if let Some(index) = words
            .iter()
            .position(|word| word.eq_ignore_ascii_case("as"))
            && let Some(name) = words.get(index + 1)
        {
            return location(name, "stage", line_number);
        }
        return words
            .get(1)
            .and_then(|name| location(name, "stage", line_number));
    }
    None
}

fn haskell_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    if trimmed.starts_with("--") {
        return None;
    }
    if let Some(symbol) = prefixed_symbol(
        trimmed,
        &[
            ("data ", "data"),
            ("newtype ", "newtype"),
            ("type ", "type"),
            ("class ", "class"),
            ("module ", "module"),
        ],
        line_number,
    ) {
        return Some(symbol);
    }
    let (name, _) = trimmed.split_once("::")?;
    location(name.trim(), "fn", line_number)
}

fn sql_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    let lower = trimmed.to_ascii_lowercase();
    for (prefix, kind) in [
        ("create table ", "table"),
        ("create view ", "view"),
        ("create function ", "fn"),
        ("create procedure ", "procedure"),
        ("create trigger ", "trigger"),
        ("create index ", "index"),
    ] {
        if lower.starts_with(prefix) {
            return location(
                identifier_prefix(trimmed[prefix.len()..].trim_start()),
                kind,
                line_number,
            );
        }
    }
    None
}

fn section_symbol(
    trimmed: &str,
    open: char,
    close: char,
    kind: &str,
    line_number: usize,
) -> Option<SymbolLocation> {
    if trimmed.starts_with(open) && trimmed.ends_with(close) {
        location(
            trimmed.trim_matches(|character| character == open || character == close),
            kind,
            line_number,
        )
    } else {
        None
    }
}

fn yaml_symbol(line: &str, line_number: usize) -> Option<SymbolLocation> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    let trimmed = line.trim();
    if trimmed.starts_with('#') || trimmed.starts_with('-') {
        return None;
    }
    let (name, _) = trimmed.split_once(':')?;
    location(name.trim(), "key", line_number)
}

fn markdown_symbol(trimmed: &str, line_number: usize) -> Option<SymbolLocation> {
    let heading = trimmed.strip_prefix('#')?;
    let level = 1 + heading
        .chars()
        .take_while(|character| *character == '#')
        .count();
    let name = heading.trim_start_matches('#').trim();
    location(name, &format!("h{level}"), line_number)
}

fn identifier_prefix(value: &str) -> &str {
    let end = value
        .char_indices()
        .find_map(|(index, character)| (!is_identifier_char(character)).then_some(index))
        .unwrap_or(value.len());
    &value[..end]
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty() && value.chars().all(is_identifier_char)
}

fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(character, '_' | ':' | '~' | '.' | '-' | '$' | '!' | '?')
}

fn starts_control_statement(value: &str) -> bool {
    let head = value
        .trim_start()
        .split(|character: char| character.is_whitespace() || character == '(')
        .next()
        .unwrap_or_default();
    matches!(
        head,
        "if" | "for" | "while" | "switch" | "catch" | "return" | "match"
    )
}

#[must_use]
pub fn find_definition(path: &str, content: &str, query: &str) -> Option<SymbolLocation> {
    let query = normalize_identifier(query);
    if query.is_empty() {
        return None;
    }
    let short_query = query.rsplit("::").next().unwrap_or(&query);
    let symbols = extract_symbols(path, content);
    symbols
        .iter()
        .find(|symbol| normalize_identifier(&symbol.name) == query)
        .or_else(|| {
            symbols
                .iter()
                .find(|symbol| normalize_identifier(&symbol.name) == short_query)
        })
        .or_else(|| {
            let query_lower = query.to_ascii_lowercase();
            let short_query_lower = short_query.to_ascii_lowercase();
            symbols.iter().find(|symbol| {
                let name = normalize_identifier(&symbol.name).to_ascii_lowercase();
                name == query_lower || name.rsplit("::").next() == Some(short_query_lower.as_str())
            })
        })
        .cloned()
}

#[must_use]
pub fn text_matches(content: &str, query: &str, limit: usize) -> Vec<(usize, String)> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    content
        .lines()
        .enumerate()
        .filter(|(_, line)| line.to_ascii_lowercase().contains(&query))
        .map(|(index, line)| (index + 1, line.trim().to_owned()))
        .take(limit)
        .collect()
}

#[must_use]
pub fn identifier_near_cursor(line: &str, preferred_column: Option<usize>) -> String {
    let tokens = identifier_tokens(line);
    if let Some(column) = preferred_column
        && let Some((_, _, token)) = tokens
            .iter()
            .find(|(start, end, _)| column >= *start && column <= *end)
    {
        return token.clone();
    }

    if let Some((_, _, token)) = tokens.iter().rev().find(|(_, end, token)| {
        if is_keyword(token) {
            return false;
        }
        line.get(end.saturating_add(1)..).is_some_and(|suffix| {
            let suffix = suffix.trim_start();
            suffix.starts_with('(')
                || suffix.starts_with("::<")
                || (suffix.starts_with('<') && suffix.contains('('))
        })
    }) {
        return token.clone();
    }

    tokens
        .into_iter()
        .rev()
        .find(|(_, _, token)| !is_keyword(token))
        .map_or_else(String::new, |(_, _, token)| token)
}

#[must_use]
pub fn is_searchable_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file_name = lower.rsplit('/').next().unwrap_or(&lower);
    if matches!(
        file_name,
        "makefile"
            | "gnumakefile"
            | "bsdmakefile"
            | "justfile"
            | "dockerfile"
            | "cmakelists.txt"
            | "gemfile"
            | "rakefile"
            | "vagrantfile"
            | "guardfile"
            | "podfile"
            | "fastfile"
            | "brewfile"
            | "capfile"
            | "berksfile"
            | "puppetfile"
            | "thorfile"
            | "sconstruct"
            | "sconscript"
            | "wscript"
            | "configure"
            | "gradlew"
            | "mvnw"
            | "jenkinsfile"
            | "build"
            | "build.bazel"
            | "workspace"
            | "workspace.bazel"
            | "procfile"
    ) || file_name.starts_with("dockerfile.")
        || file_name.starts_with("makefile.")
    {
        return true;
    }

    let extension = file_name.rsplit_once('.').map(|(_, extension)| extension);
    extension.is_some_and(|extension| {
        matches!(
            extension,
            "rs" | "c"
                | "h"
                | "cc"
                | "cpp"
                | "cxx"
                | "hpp"
                | "hh"
                | "hxx"
                | "cs"
                | "java"
                | "kt"
                | "kts"
                | "swift"
                | "go"
                | "py"
                | "pyi"
                | "pyw"
                | "rb"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "js"
                | "jsx"
                | "mjs"
                | "cjs"
                | "ts"
                | "tsx"
                | "mts"
                | "cts"
                | "json"
                | "jsonc"
                | "yaml"
                | "yml"
                | "toml"
                | "sql"
                | "html"
                | "htm"
                | "xml"
                | "svg"
                | "css"
                | "scss"
                | "sass"
                | "less"
                | "md"
                | "markdown"
                | "mdx"
                | "lua"
                | "hs"
                | "lhs"
                | "mk"
                | "mak"
                | "cmake"
                | "bzl"
                | "bazel"
                | "groovy"
                | "gradle"
                | "pl"
                | "pm"
                | "php"
                | "phtml"
        )
    }) || !file_name.contains('.')
}

fn identifier_tokens(line: &str) -> Vec<(usize, usize, String)> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, character) in line.char_indices() {
        if is_cursor_identifier_char(character) {
            start.get_or_insert(index);
        } else if let Some(token_start) = start.take() {
            let token = line[token_start..index].trim_matches(':');
            if !token.is_empty() {
                tokens.push((token_start, index.saturating_sub(1), token.to_owned()));
            }
        }
    }
    if let Some(token_start) = start {
        let token = line[token_start..].trim_matches(':');
        if !token.is_empty() {
            tokens.push((token_start, line.len().saturating_sub(1), token.to_owned()));
        }
    }
    tokens
}

fn is_cursor_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | ':' | '$' | '!' | '?')
}

fn normalize_identifier(value: &str) -> String {
    value
        .trim()
        .trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && !matches!(character, '_' | ':' | '$' | '!' | '?')
        })
        .to_owned()
}

fn is_keyword(value: &str) -> bool {
    matches!(
        value,
        "pub"
            | "let"
            | "mut"
            | "self"
            | "Self"
            | "return"
            | "const"
            | "static"
            | "impl"
            | "struct"
            | "class"
            | "function"
            | "async"
            | "await"
            | "fn"
            | "def"
            | "var"
            | "val"
            | "type"
            | "enum"
            | "trait"
            | "interface"
    )
}

#[cfg(test)]
mod tests {
    use super::{DetectedLanguage, detect_language, extract_symbols};

    #[test]
    fn detects_special_filenames_before_extensions() {
        assert_eq!(
            detect_language("Makefile", "all:\n\techo ok"),
            DetectedLanguage::Make
        );
        assert_eq!(
            detect_language("docker/Dockerfile.dev", "FROM rust AS build"),
            DetectedLanguage::Dockerfile
        );
        assert_eq!(
            detect_language("CMakeLists.txt", "project(repotrek)"),
            DetectedLanguage::CMake
        );
        assert_eq!(
            detect_language("Gemfile", "source 'x'"),
            DetectedLanguage::Ruby
        );
    }

    #[test]
    fn detects_extensionless_shebang() {
        assert_eq!(
            detect_language(
                "scripts/release",
                "#!/usr/bin/env python3\ndef main():\n    pass"
            ),
            DetectedLanguage::Python
        );
    }

    #[test]
    fn extracts_make_targets() {
        let symbols = extract_symbols(
            "Makefile",
            "CC := cc\nall: build test\n\t@echo ok\nclean:\n",
        );
        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].name, "CC");
        assert_eq!(symbols[1].name, "all");
        assert_eq!(symbols[2].name, "clean");
    }

    #[test]
    fn extracts_rust_and_javascript_symbols() {
        let rust = extract_symbols(
            "src/main.rs",
            "pub(crate) async fn run() {}\nstruct App {}\n",
        );
        assert_eq!(rust[0].name, "run");
        assert_eq!(rust[1].name, "App");

        let js = extract_symbols(
            "app.ts",
            "export const load = async () => {}\nexport class App {}\n",
        );
        assert_eq!(js[0].name, "load");
        assert_eq!(js[1].name, "App");
    }

    #[test]
    fn finds_exact_definition_and_text_matches() {
        let content = "fn helper() {}\nfn main() { helper(); }\n";
        let definition = super::find_definition("main.rs", content, "helper").expect("definition");
        assert_eq!(definition.line, 1);
        assert_eq!(super::text_matches(content, "helper", 10).len(), 2);
    }

    #[test]
    fn chooses_called_identifier_on_a_source_line() {
        assert_eq!(
            super::identifier_near_cursor("let result = service.load_item(id);", None),
            "load_item"
        );
        assert_eq!(
            super::identifier_near_cursor("crate::cache::read_entry(key)?;", None),
            "crate::cache::read_entry"
        );
    }

    #[test]
    fn does_not_treat_names_starting_with_control_words_as_control_statements() {
        let symbols = extract_symbols(
            "Formatter.java",
            "public String format(Object value) {\n    return value.toString();\n}\n",
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "format");
    }
}
