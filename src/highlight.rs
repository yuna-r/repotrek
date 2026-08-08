use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

const KEYWORD: Color = Color::Rgb(255, 123, 114);
const TYPE: Color = Color::Rgb(121, 192, 255);
const STRING: Color = Color::Rgb(165, 214, 255);
const NUMBER: Color = Color::Rgb(121, 192, 255);
const COMMENT: Color = Color::Rgb(139, 148, 158);
const FUNCTION: Color = Color::Rgb(210, 168, 255);
const MACRO: Color = Color::Rgb(255, 166, 87);
const CONSTANT: Color = Color::Rgb(255, 166, 87);
const PUNCT: Color = Color::Rgb(201, 209, 217);

#[must_use]
pub fn source_spans(line: &str, extension: &str) -> Vec<Span<'static>> {
    let expanded = line.replace('\t', "    ");
    if expanded.trim_start().starts_with('#')
        && matches!(extension, "c" | "h" | "cc" | "cpp" | "cxx" | "hpp")
    {
        return vec![Span::styled(
            expanded,
            Style::new().fg(MACRO).add_modifier(Modifier::BOLD),
        )];
    }

    let marker = comment_marker(extension);
    let comment_index = marker.and_then(|marker| find_comment(&expanded, marker));
    let (code, comment) = comment_index.map_or((expanded.as_str(), None), |index| {
        (&expanded[..index], Some(&expanded[index..]))
    });

    let mut spans = tokenize(code);
    if let Some(comment) = comment {
        spans.push(Span::styled(
            comment.to_owned(),
            Style::new().fg(COMMENT).add_modifier(Modifier::ITALIC),
        ));
    }
    spans
}

#[must_use]
pub fn source_html(line: &str, extension: &str) -> String {
    source_spans(line, extension)
        .into_iter()
        .map(|span| {
            let content = escape_html(span.content.as_ref());
            let color = html_color(span.style.fg);
            let mut css = format!("color:{color}");
            if span.style.add_modifier.contains(Modifier::BOLD) {
                css.push_str(";font-weight:650");
            }
            if span.style.add_modifier.contains(Modifier::ITALIC) {
                css.push_str(";font-style:italic");
            }
            format!("<span style=\"{css}\">{content}</span>")
        })
        .collect()
}

fn tokenize(code: &str) -> Vec<Span<'static>> {
    let chars = code.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let character = chars[index];

        if matches!(character, '"' | '\'') {
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
                Style::new().fg(STRING),
            ));
            continue;
        }

        if character.is_ascii_alphabetic() || character == '_' {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
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
                token_style(&token, next_non_space),
            ));
            continue;
        }

        if character.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric()
                    || matches!(chars[index], '.' | '_' | 'x' | 'X'))
            {
                index += 1;
            }
            spans.push(Span::styled(
                chars[start..index].iter().collect::<String>(),
                Style::new().fg(NUMBER),
            ));
            continue;
        }

        let start = index;
        index += 1;
        while index < chars.len()
            && !matches!(chars[index], '"' | '\'')
            && !chars[index].is_ascii_alphanumeric()
            && chars[index] != '_'
        {
            index += 1;
        }
        spans.push(Span::styled(
            chars[start..index].iter().collect::<String>(),
            Style::new().fg(PUNCT),
        ));
    }
    spans
}

fn token_style(token: &str, next_non_space: Option<char>) -> Style {
    if matches!(
        token,
        "true" | "false" | "None" | "null" | "nil" | "Some" | "Ok" | "Err"
    ) {
        return Style::new().fg(CONSTANT);
    }
    if is_keyword(token) {
        return Style::new().fg(KEYWORD).add_modifier(Modifier::BOLD);
    }
    if is_builtin_type(token) || token.chars().next().is_some_and(char::is_uppercase) {
        return Style::new().fg(TYPE);
    }
    if next_non_space == Some('(') {
        return Style::new().fg(FUNCTION);
    }
    if token
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit())
        && token.chars().any(char::is_alphabetic)
    {
        return Style::new().fg(CONSTANT);
    }
    Style::new().fg(Color::Rgb(230, 237, 243))
}

fn is_builtin_type(token: &str) -> bool {
    matches!(
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
            | "int"
            | "long"
            | "float"
            | "double"
            | "void"
            | "bytes"
            | "dict"
            | "list"
            | "tuple"
    )
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token,
        "as" | "async"
            | "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "crate"
            | "def"
            | "defer"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "except"
            | "export"
            | "extends"
            | "extern"
            | "finally"
            | "fn"
            | "for"
            | "from"
            | "func"
            | "function"
            | "if"
            | "impl"
            | "import"
            | "in"
            | "interface"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "namespace"
            | "new"
            | "package"
            | "pass"
            | "private"
            | "protected"
            | "pub"
            | "public"
            | "raise"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "switch"
            | "throw"
            | "trait"
            | "try"
            | "type"
            | "typeof"
            | "unsafe"
            | "use"
            | "var"
            | "where"
            | "while"
            | "with"
            | "yield"
    )
}

fn comment_marker(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "rs" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "java" | "js" | "jsx" | "ts" | "tsx"
        | "go" | "swift" | "kt" | "kts" | "cs" | "scala" => Some("//"),
        "py" | "rb" | "sh" | "bash" | "zsh" | "fish" | "yaml" | "yml" | "toml" | "r" | "pl" => {
            Some("#")
        }
        "sql" | "lua" | "hs" => Some("--"),
        _ => None,
    }
}

fn find_comment(line: &str, marker: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let marker = marker.as_bytes();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(byte, b'"' | b'\'') {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(marker) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn html_color(color: Option<Color>) -> &'static str {
    match color {
        Some(Color::Rgb(255, 123, 114)) => "#cf222e",
        Some(Color::Rgb(121, 192, 255)) => "#0550ae",
        Some(Color::Rgb(165, 214, 255)) => "#0a3069",
        Some(Color::Rgb(139, 148, 158)) => "#6e7781",
        Some(Color::Rgb(210, 168, 255)) => "#8250df",
        Some(Color::Rgb(255, 166, 87)) => "#953800",
        Some(Color::Rgb(201, 209, 217)) => "#24292f",
        _ => "#24292f",
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
