use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::{
    language::{DetectedLanguage, detect_language},
    theme::Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
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
    Other,
}

#[must_use]
pub fn source_spans(line: &str, path: &str, theme: Theme) -> Vec<Span<'static>> {
    source_spans_with_language(line, detect_language(path, line), theme)
}

#[must_use]
pub fn source_spans_with_language(
    line: &str,
    language: DetectedLanguage,
    theme: Theme,
) -> Vec<Span<'static>> {
    let language = Language::from(language);
    let expanded = line.replace('\t', "    ");

    if is_preprocessor(&expanded, language) {
        return vec![Span::styled(
            expanded,
            Style::new().fg(theme.constant).add_modifier(Modifier::BOLD),
        )];
    }

    if matches!(language, Language::Markdown) {
        return markdown_spans(&expanded, theme);
    }
    if matches!(language, Language::Json) {
        return tokenize_json(&expanded, theme);
    }
    if matches!(language, Language::Yaml | Language::Toml) {
        return tokenize_config(&expanded, language, theme);
    }

    let comment_index =
        line_comment_marker(language).and_then(|marker| find_comment(&expanded, marker));
    let (code, comment) = comment_index.map_or((expanded.as_str(), None), |index| {
        (&expanded[..index], Some(&expanded[index..]))
    });

    let mut spans = tokenize_code(code, language, theme);
    if let Some(comment) = comment {
        spans.push(Span::styled(
            comment.to_owned(),
            Style::new()
                .fg(theme.comment)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    spans
}

#[must_use]
pub fn source_html(line: &str, path: &str) -> String {
    source_html_with_language(line, detect_language(path, line))
}

#[must_use]
pub fn source_html_with_language(line: &str, language: DetectedLanguage) -> String {
    let theme = Theme::light();
    source_spans_with_language(line, language, theme)
        .into_iter()
        .map(|span| {
            let content = escape_html(span.content.as_ref());
            let color = html_color(span.style.fg, theme.text);
            let mut css = format!("color:{color}");
            if span.style.add_modifier.contains(Modifier::BOLD) {
                css.push_str(";font-weight:650");
            }
            if span.style.add_modifier.contains(Modifier::ITALIC) {
                css.push_str(";font-style:italic");
            }
            format!("<span style=\"{css}\">{content}</span>")
        })
        .collect::<Vec<_>>()
        .join("")
}

fn tokenize_code(code: &str, language: Language, theme: Theme) -> Vec<Span<'static>> {
    let chars = code.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let character = chars[index];

        if matches!(character, '"' | '\'') || (character == '`' && supports_backticks(language)) {
            let quote = character;
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < chars.len() {
                let current = chars[index];
                index += 1;
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == quote {
                    break;
                }
            }
            spans.push(Span::styled(
                chars[start..index].iter().collect::<String>(),
                Style::new().fg(theme.string),
            ));
            continue;
        }

        if character.is_alphabetic() || character == '_' || character == '$' {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_alphanumeric() || matches!(chars[index], '_' | '$' | '!' | '?'))
            {
                index += 1;
            }
            let token = chars[start..index].iter().collect::<String>();
            let next_non_space = chars[index..]
                .iter()
                .copied()
                .find(|ch| !ch.is_whitespace());
            spans.push(Span::styled(
                token.clone(),
                token_style(&token, next_non_space, language, theme),
            ));
            continue;
        }

        if character.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric()
                    || matches!(chars[index], '.' | '_' | '+' | '-'))
            {
                index += 1;
            }
            spans.push(Span::styled(
                chars[start..index].iter().collect::<String>(),
                Style::new().fg(theme.number),
            ));
            continue;
        }

        let start = index;
        index += 1;
        while index < chars.len()
            && !matches!(chars[index], '"' | '\'' | '`')
            && !chars[index].is_alphanumeric()
            && !matches!(chars[index], '_' | '$')
        {
            index += 1;
        }
        spans.push(Span::styled(
            chars[start..index].iter().collect::<String>(),
            Style::new().fg(theme.punctuation),
        ));
    }
    spans
}

fn tokenize_json(line: &str, theme: Theme) -> Vec<Span<'static>> {
    let mut spans = tokenize_code(line, Language::Json, theme);
    // JSON property names are strings followed by a colon. The generic tokenizer already
    // highlights strings; booleans and null are handled as constants.
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), Style::new().fg(theme.text)));
    }
    spans
}

fn tokenize_config(line: &str, language: Language, theme: Theme) -> Vec<Span<'static>> {
    let marker = "#";
    let comment_index = find_comment(line, marker);
    let (code, comment) =
        comment_index.map_or((line, None), |index| (&line[..index], Some(&line[index..])));
    let separator = if language == Language::Yaml { ':' } else { '=' };
    let mut spans = Vec::new();
    if let Some(index) = code.find(separator) {
        let key = &code[..index];
        spans.push(Span::styled(
            key.to_owned(),
            Style::new().fg(theme.function).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            separator.to_string(),
            Style::new().fg(theme.punctuation),
        ));
        spans.extend(tokenize_code(
            &code[index + separator.len_utf8()..],
            language,
            theme,
        ));
    } else {
        spans.extend(tokenize_code(code, language, theme));
    }
    if let Some(comment) = comment {
        spans.push(Span::styled(
            comment.to_owned(),
            Style::new()
                .fg(theme.comment)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    spans
}

fn markdown_spans(line: &str, theme: Theme) -> Vec<Span<'static>> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return vec![Span::styled(
            line.to_owned(),
            Style::new().fg(theme.function).add_modifier(Modifier::BOLD),
        )];
    }
    if trimmed.starts_with('>') {
        return vec![Span::styled(
            line.to_owned(),
            Style::new()
                .fg(theme.comment)
                .add_modifier(Modifier::ITALIC),
        )];
    }
    if trimmed.starts_with("```") {
        return vec![Span::styled(
            line.to_owned(),
            Style::new().fg(theme.constant),
        )];
    }
    vec![Span::styled(line.to_owned(), Style::new().fg(theme.text))]
}

fn token_style(
    token: &str,
    next_non_space: Option<char>,
    language: Language,
    theme: Theme,
) -> Style {
    if is_constant(token, language) {
        return Style::new().fg(theme.constant);
    }
    if is_keyword(token, language) {
        return Style::new().fg(theme.keyword).add_modifier(Modifier::BOLD);
    }
    if is_builtin_type(token, language) || token.chars().next().is_some_and(char::is_uppercase) {
        return Style::new().fg(theme.type_name);
    }
    if next_non_space == Some('(') {
        return Style::new().fg(theme.function);
    }
    if token
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit())
        && token.chars().any(char::is_alphabetic)
    {
        return Style::new().fg(theme.constant);
    }
    Style::new().fg(theme.text)
}

fn is_constant(token: &str, language: Language) -> bool {
    match language {
        Language::Python => matches!(token, "True" | "False" | "None"),
        Language::Ruby => matches!(token, "true" | "false" | "nil"),
        Language::Rust => matches!(token, "true" | "false" | "Some" | "None" | "Ok" | "Err"),
        _ => matches!(token, "true" | "false" | "null" | "nil"),
    }
}

fn is_builtin_type(token: &str, language: Language) -> bool {
    match language {
        Language::Rust => matches!(
            token,
            "bool"
                | "char"
                | "str"
                | "String"
                | "usize"
                | "isize"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "f32"
                | "f64"
                | "Self"
                | "Option"
                | "Result"
                | "Vec"
                | "Box"
        ),
        Language::C | Language::Cpp => matches!(
            token,
            "void"
                | "char"
                | "short"
                | "int"
                | "long"
                | "float"
                | "double"
                | "signed"
                | "unsigned"
                | "size_t"
                | "bool"
                | "wchar_t"
                | "auto"
        ),
        Language::CSharp => matches!(
            token,
            "bool"
                | "byte"
                | "sbyte"
                | "char"
                | "decimal"
                | "double"
                | "float"
                | "int"
                | "uint"
                | "long"
                | "ulong"
                | "short"
                | "ushort"
                | "object"
                | "string"
        ),
        Language::Java | Language::Kotlin => matches!(
            token,
            "boolean"
                | "byte"
                | "char"
                | "double"
                | "float"
                | "int"
                | "long"
                | "short"
                | "void"
                | "String"
                | "Any"
                | "Unit"
        ),
        Language::Go => matches!(
            token,
            "bool"
                | "byte"
                | "complex64"
                | "complex128"
                | "error"
                | "float32"
                | "float64"
                | "int"
                | "int8"
                | "int16"
                | "int32"
                | "int64"
                | "rune"
                | "string"
                | "uint"
                | "uint8"
                | "uint16"
                | "uint32"
                | "uint64"
                | "uintptr"
        ),
        Language::Python => matches!(
            token,
            "str" | "int" | "float" | "bool" | "bytes" | "dict" | "list" | "tuple" | "set"
        ),
        Language::TypeScript => matches!(
            token,
            "string" | "number" | "boolean" | "unknown" | "never" | "any" | "void"
        ),
        _ => false,
    }
}

fn is_keyword(token: &str, language: Language) -> bool {
    match language {
        Language::Rust => matches!(
            token,
            "as" | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "yield"
        ),
        Language::C | Language::Cpp => matches!(
            token,
            "alignas"
                | "alignof"
                | "asm"
                | "auto"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "constexpr"
                | "continue"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "enum"
                | "explicit"
                | "export"
                | "extern"
                | "for"
                | "friend"
                | "goto"
                | "if"
                | "inline"
                | "mutable"
                | "namespace"
                | "new"
                | "noexcept"
                | "operator"
                | "private"
                | "protected"
                | "public"
                | "register"
                | "return"
                | "sizeof"
                | "static"
                | "struct"
                | "switch"
                | "template"
                | "this"
                | "throw"
                | "try"
                | "typedef"
                | "typename"
                | "union"
                | "using"
                | "virtual"
                | "volatile"
                | "while"
        ),
        Language::CSharp => matches!(
            token,
            "abstract"
                | "as"
                | "async"
                | "await"
                | "base"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "delegate"
                | "do"
                | "else"
                | "enum"
                | "event"
                | "explicit"
                | "extern"
                | "finally"
                | "fixed"
                | "for"
                | "foreach"
                | "if"
                | "implicit"
                | "in"
                | "interface"
                | "internal"
                | "is"
                | "lock"
                | "namespace"
                | "new"
                | "operator"
                | "out"
                | "override"
                | "params"
                | "private"
                | "protected"
                | "public"
                | "readonly"
                | "ref"
                | "return"
                | "sealed"
                | "static"
                | "struct"
                | "switch"
                | "this"
                | "throw"
                | "try"
                | "typeof"
                | "using"
                | "virtual"
                | "volatile"
                | "while"
                | "yield"
        ),
        Language::Java => matches!(
            token,
            "abstract"
                | "assert"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "do"
                | "else"
                | "enum"
                | "extends"
                | "final"
                | "finally"
                | "for"
                | "goto"
                | "if"
                | "implements"
                | "import"
                | "instanceof"
                | "interface"
                | "native"
                | "new"
                | "package"
                | "private"
                | "protected"
                | "public"
                | "return"
                | "static"
                | "strictfp"
                | "super"
                | "switch"
                | "synchronized"
                | "this"
                | "throw"
                | "throws"
                | "transient"
                | "try"
                | "volatile"
                | "while"
        ),
        Language::Kotlin => matches!(
            token,
            "as" | "break"
                | "class"
                | "continue"
                | "do"
                | "else"
                | "false"
                | "for"
                | "fun"
                | "if"
                | "in"
                | "interface"
                | "is"
                | "null"
                | "object"
                | "package"
                | "return"
                | "super"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "typealias"
                | "typeof"
                | "val"
                | "var"
                | "when"
                | "while"
        ),
        Language::Swift => matches!(
            token,
            "associatedtype"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "continue"
                | "default"
                | "defer"
                | "deinit"
                | "do"
                | "else"
                | "enum"
                | "extension"
                | "fallthrough"
                | "for"
                | "func"
                | "guard"
                | "if"
                | "import"
                | "in"
                | "init"
                | "inout"
                | "internal"
                | "let"
                | "open"
                | "operator"
                | "private"
                | "protocol"
                | "public"
                | "repeat"
                | "return"
                | "static"
                | "struct"
                | "subscript"
                | "switch"
                | "throw"
                | "throws"
                | "try"
                | "typealias"
                | "var"
                | "where"
                | "while"
        ),
        Language::Go => matches!(
            token,
            "break"
                | "case"
                | "chan"
                | "const"
                | "continue"
                | "default"
                | "defer"
                | "else"
                | "fallthrough"
                | "for"
                | "func"
                | "go"
                | "goto"
                | "if"
                | "import"
                | "interface"
                | "map"
                | "package"
                | "range"
                | "return"
                | "select"
                | "struct"
                | "switch"
                | "type"
                | "var"
        ),
        Language::Python => matches!(
            token,
            "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "global"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "nonlocal"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        ),
        Language::Ruby => matches!(
            token,
            "alias"
                | "and"
                | "begin"
                | "break"
                | "case"
                | "class"
                | "def"
                | "defined"
                | "do"
                | "else"
                | "elsif"
                | "end"
                | "ensure"
                | "for"
                | "if"
                | "in"
                | "module"
                | "next"
                | "not"
                | "or"
                | "redo"
                | "rescue"
                | "retry"
                | "return"
                | "super"
                | "then"
                | "undef"
                | "unless"
                | "until"
                | "when"
                | "while"
                | "yield"
        ),
        Language::Shell => matches!(
            token,
            "case"
                | "do"
                | "done"
                | "elif"
                | "else"
                | "esac"
                | "fi"
                | "for"
                | "function"
                | "if"
                | "in"
                | "select"
                | "then"
                | "time"
                | "until"
                | "while"
        ),
        Language::JavaScript | Language::TypeScript => matches!(
            token,
            "as" | "async"
                | "await"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "debugger"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "enum"
                | "export"
                | "extends"
                | "finally"
                | "for"
                | "from"
                | "function"
                | "get"
                | "if"
                | "implements"
                | "import"
                | "in"
                | "instanceof"
                | "interface"
                | "let"
                | "new"
                | "of"
                | "package"
                | "private"
                | "protected"
                | "public"
                | "return"
                | "set"
                | "static"
                | "super"
                | "switch"
                | "throw"
                | "try"
                | "type"
                | "typeof"
                | "var"
                | "void"
                | "while"
                | "with"
                | "yield"
        ),
        Language::Sql => matches!(
            token.to_ascii_uppercase().as_str(),
            "SELECT"
                | "FROM"
                | "WHERE"
                | "INSERT"
                | "INTO"
                | "UPDATE"
                | "DELETE"
                | "CREATE"
                | "ALTER"
                | "DROP"
                | "TABLE"
                | "INDEX"
                | "JOIN"
                | "LEFT"
                | "RIGHT"
                | "INNER"
                | "OUTER"
                | "ON"
                | "AS"
                | "AND"
                | "OR"
                | "NOT"
                | "NULL"
                | "VALUES"
                | "GROUP"
                | "BY"
                | "ORDER"
                | "HAVING"
                | "LIMIT"
                | "OFFSET"
                | "UNION"
        ),
        Language::Lua => matches!(
            token,
            "and"
                | "break"
                | "do"
                | "else"
                | "elseif"
                | "end"
                | "false"
                | "for"
                | "function"
                | "goto"
                | "if"
                | "in"
                | "local"
                | "nil"
                | "not"
                | "or"
                | "repeat"
                | "return"
                | "then"
                | "true"
                | "until"
                | "while"
        ),
        Language::Haskell => matches!(
            token,
            "case"
                | "class"
                | "data"
                | "default"
                | "deriving"
                | "do"
                | "else"
                | "foreign"
                | "if"
                | "import"
                | "in"
                | "infix"
                | "infixl"
                | "infixr"
                | "instance"
                | "let"
                | "module"
                | "newtype"
                | "of"
                | "then"
                | "type"
                | "where"
        ),
        Language::Json
        | Language::Yaml
        | Language::Toml
        | Language::Html
        | Language::Css
        | Language::Markdown
        | Language::Other => false,
    }
}

impl From<DetectedLanguage> for Language {
    fn from(language: DetectedLanguage) -> Self {
        match language {
            DetectedLanguage::Rust => Self::Rust,
            DetectedLanguage::C => Self::C,
            DetectedLanguage::Cpp => Self::Cpp,
            DetectedLanguage::CSharp => Self::CSharp,
            DetectedLanguage::Java => Self::Java,
            DetectedLanguage::Kotlin => Self::Kotlin,
            DetectedLanguage::Swift => Self::Swift,
            DetectedLanguage::Go => Self::Go,
            DetectedLanguage::Python | DetectedLanguage::Starlark => Self::Python,
            DetectedLanguage::Ruby | DetectedLanguage::Perl => Self::Ruby,
            DetectedLanguage::Shell
            | DetectedLanguage::Make
            | DetectedLanguage::Dockerfile
            | DetectedLanguage::CMake => Self::Shell,
            DetectedLanguage::JavaScript => Self::JavaScript,
            DetectedLanguage::TypeScript => Self::TypeScript,
            DetectedLanguage::Json => Self::Json,
            DetectedLanguage::Yaml => Self::Yaml,
            DetectedLanguage::Toml => Self::Toml,
            DetectedLanguage::Sql => Self::Sql,
            DetectedLanguage::Html => Self::Html,
            DetectedLanguage::Css => Self::Css,
            DetectedLanguage::Markdown => Self::Markdown,
            DetectedLanguage::Lua => Self::Lua,
            DetectedLanguage::Haskell => Self::Haskell,
            DetectedLanguage::Groovy => Self::Java,
            DetectedLanguage::Php => Self::JavaScript,
            DetectedLanguage::Other => Self::Other,
        }
    }
}

fn line_comment_marker(language: Language) -> Option<&'static str> {
    match language {
        Language::Rust
        | Language::C
        | Language::Cpp
        | Language::CSharp
        | Language::Java
        | Language::Kotlin
        | Language::Swift
        | Language::Go
        | Language::JavaScript
        | Language::TypeScript => Some("//"),
        Language::Python | Language::Ruby | Language::Shell | Language::Yaml | Language::Toml => {
            Some("#")
        }
        Language::Sql | Language::Lua | Language::Haskell => Some("--"),
        Language::Json | Language::Html | Language::Css | Language::Markdown | Language::Other => {
            None
        }
    }
}

fn is_preprocessor(line: &str, language: Language) -> bool {
    line.trim_start().starts_with('#') && matches!(language, Language::C | Language::Cpp)
}

fn supports_backticks(language: Language) -> bool {
    matches!(
        language,
        Language::JavaScript | Language::TypeScript | Language::Shell
    )
}

fn find_comment(line: &str, marker: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let bytes = line.as_bytes();
    let marker = marker.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let current = bytes[index] as char;
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if current == '\\' {
            escaped = true;
            index += 1;
            continue;
        }
        if matches!(current, '"' | '\'' | '`') {
            if quote == Some(current) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(current);
            }
            index += 1;
            continue;
        }
        if quote.is_none()
            && index + marker.len() <= bytes.len()
            && &bytes[index..index + marker.len()] == marker
        {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn html_color(color: Option<ratatui::style::Color>, fallback: ratatui::style::Color) -> String {
    let color = color.unwrap_or(fallback);
    match color {
        ratatui::style::Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
        _ => "#1f2328".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::source_spans;
    use crate::{
        language::{DetectedLanguage, detect_language},
        theme::Theme,
    };

    #[test]
    fn detects_common_languages() {
        assert_eq!(detect_language("src/main.rs", ""), DetectedLanguage::Rust);
        assert_eq!(detect_language("app.py", ""), DetectedLanguage::Python);
        assert_eq!(detect_language("web.tsx", ""), DetectedLanguage::TypeScript);
        assert_eq!(detect_language("main.go", ""), DetectedLanguage::Go);
        assert_eq!(detect_language("config.yaml", ""), DetectedLanguage::Yaml);
        assert_eq!(
            detect_language("Makefile", "all:\n\tcargo build\n"),
            DetectedLanguage::Make
        );
    }

    #[test]
    fn highlights_keywords_per_language() {
        let rust = source_spans("pub fn main() {}", "src/main.rs", Theme::dark());
        let python = source_spans("def main():", "main.py", Theme::light());
        assert!(rust.len() > 1);
        assert!(python.len() > 1);
    }
}
