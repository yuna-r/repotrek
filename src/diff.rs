#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Hunk,
    Context,
    Add,
    Delete,
    Meta,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub kind: DiffKind,
    pub text: String,
}

#[must_use]
pub fn parse_patch(patch: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut old_line = 0usize;
    let mut new_line = 0usize;

    for raw in patch.lines() {
        if raw.starts_with("@@") {
            if let Some((old_start, new_start)) = parse_hunk_header(raw) {
                old_line = old_start;
                new_line = new_start;
            }
            lines.push(DiffLine {
                old_line: None,
                new_line: None,
                kind: DiffKind::Hunk,
                text: raw.to_owned(),
            });
            continue;
        }

        if raw.starts_with('+') && !raw.starts_with("+++") {
            lines.push(DiffLine {
                old_line: None,
                new_line: Some(new_line),
                kind: DiffKind::Add,
                text: raw.get(1..).unwrap_or_default().to_owned(),
            });
            new_line = new_line.saturating_add(1);
        } else if raw.starts_with('-') && !raw.starts_with("---") {
            lines.push(DiffLine {
                old_line: Some(old_line),
                new_line: None,
                kind: DiffKind::Delete,
                text: raw.get(1..).unwrap_or_default().to_owned(),
            });
            old_line = old_line.saturating_add(1);
        } else if raw.starts_with(' ') {
            lines.push(DiffLine {
                old_line: Some(old_line),
                new_line: Some(new_line),
                kind: DiffKind::Context,
                text: raw.get(1..).unwrap_or_default().to_owned(),
            });
            old_line = old_line.saturating_add(1);
            new_line = new_line.saturating_add(1);
        } else {
            lines.push(DiffLine {
                old_line: None,
                new_line: None,
                kind: DiffKind::Meta,
                text: raw.to_owned(),
            });
        }
    }
    lines
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    let end = line.get(2..)?.find("@@")? + 2;
    let body = line.get(2..end)?.trim();
    let mut parts = body.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    Some((parse_range_start(old)?, parse_range_start(new)?))
}

fn parse_range_start(value: &str) -> Option<usize> {
    value.split(',').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{DiffKind, parse_patch};

    #[test]
    fn assigns_unified_diff_line_numbers() {
        let patch = "@@ -10,2 +10,3 @@\n old\n-gone\n+new\n+extra";
        let lines = parse_patch(patch);
        assert_eq!(lines[1].old_line, Some(10));
        assert_eq!(lines[1].new_line, Some(10));
        assert_eq!(lines[2].old_line, Some(11));
        assert_eq!(lines[2].new_line, None);
        assert_eq!(lines[3].old_line, None);
        assert_eq!(lines[3].new_line, Some(11));
        assert_eq!(lines[4].new_line, Some(12));
        assert_eq!(lines[3].kind, DiffKind::Add);
    }
}
