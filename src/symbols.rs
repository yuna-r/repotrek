use crate::model::SymbolLocation;

#[must_use]
pub fn extract_symbols(path: &str, content: &str) -> Vec<SymbolLocation> {
    let extension = path.rsplit_once('.').map_or("", |(_, extension)| extension);
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| symbol_from_line(extension, line, index + 1))
        .collect()
}

fn symbol_from_line(extension: &str, line: &str, line_number: usize) -> Option<SymbolLocation> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
        return None;
    }

    let candidates: &[(&str, &str)] = match extension {
        "rs" => &[
            ("pub async fn ", "fn"),
            ("pub(crate) async fn ", "fn"),
            ("async fn ", "fn"),
            ("pub fn ", "fn"),
            ("pub(crate) fn ", "fn"),
            ("fn ", "fn"),
            ("pub struct ", "struct"),
            ("struct ", "struct"),
            ("pub enum ", "enum"),
            ("enum ", "enum"),
            ("pub trait ", "trait"),
            ("trait ", "trait"),
            ("impl ", "impl"),
            ("pub mod ", "mod"),
            ("mod ", "mod"),
        ],
        "py" => &[("async def ", "fn"), ("def ", "fn"), ("class ", "class")],
        "go" => &[("func ", "fn"), ("type ", "type")],
        "js" | "jsx" | "ts" | "tsx" => &[
            ("export async function ", "fn"),
            ("export function ", "fn"),
            ("async function ", "fn"),
            ("function ", "fn"),
            ("export class ", "class"),
            ("class ", "class"),
            ("export interface ", "interface"),
            ("interface ", "interface"),
            ("export type ", "type"),
            ("type ", "type"),
        ],
        "java" | "kt" | "kts" | "cs" | "swift" => &[
            ("class ", "class"),
            ("interface ", "interface"),
            ("struct ", "struct"),
            ("enum ", "enum"),
        ],
        _ => &[],
    };

    for (prefix, kind) in candidates {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name = identifier_prefix(rest);
            if !name.is_empty() {
                return Some(SymbolLocation {
                    name: name.to_owned(),
                    kind: (*kind).to_owned(),
                    line: line_number,
                });
            }
        }
    }

    if matches!(extension, "c" | "h" | "cc" | "cpp" | "cxx" | "hpp")
        && trimmed.contains('(')
        && (trimmed.ends_with('{') || trimmed.ends_with(')'))
        && !starts_control_statement(trimmed)
    {
        let before_paren = trimmed.split('(').next()?.trim_end();
        let name = before_paren
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .trim_matches(|ch: char| ch == '*' || ch == '&');
        if is_identifier(name) {
            return Some(SymbolLocation {
                name: name.to_owned(),
                kind: "fn".to_owned(),
                line: line_number,
            });
        }
    }

    None
}

fn identifier_prefix(value: &str) -> &str {
    let end = value
        .char_indices()
        .find_map(|(index, ch)| (!is_identifier_char(ch)).then_some(index))
        .unwrap_or(value.len());
    &value[..end]
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty() && value.chars().all(is_identifier_char)
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '~'
}

fn starts_control_statement(value: &str) -> bool {
    ["if", "for", "while", "switch", "catch", "return"]
        .into_iter()
        .any(|word| value.starts_with(word))
}
