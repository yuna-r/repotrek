use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

#[must_use]
pub fn source_spans(line: &str, extension: &str) -> Vec<Span<'static>> {
    let expanded = line.replace('\t', "    ");
    let marker = comment_marker(extension);
    let comment_index = marker.and_then(|marker| find_comment(&expanded, marker));
    let (code, comment) = comment_index.map_or((expanded.as_str(), None), |index| {
        (&expanded[..index], Some(&expanded[index..]))
    });

    let mut spans = tokenize(code);
    if let Some(comment) = comment {
        spans.push(Span::styled(
            comment.to_owned(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    spans
}

fn tokenize(code: &str) -> Vec<Span<'static>> {
    let chars: Vec<char> = code.chars().collect();
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
                Style::default().fg(Color::Green),
            ));
            continue;
        }

        if character.is_ascii_alphanumeric() || character == '_' {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
            }
            let token = chars[start..index].iter().collect::<String>();
            let style = token_style(&token);
            spans.push(Span::styled(token, style));
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
        spans.push(Span::raw(chars[start..index].iter().collect::<String>()));
    }

    spans
}

fn token_style(token: &str) -> Style {
    if token.chars().all(|character| character.is_ascii_digit()) {
        return Style::default().fg(Color::Yellow);
    }
    if matches!(token, "true" | "false" | "None" | "null" | "nil") {
        return Style::default().fg(Color::LightMagenta);
    }
    if is_keyword(token) {
        return Style::default()
            .fg(Color::LightMagenta)
            .add_modifier(Modifier::BOLD);
    }
    if token.chars().next().is_some_and(char::is_uppercase) {
        return Style::default().fg(Color::LightBlue);
    }
    Style::default()
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
