use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::Local;

use crate::model::{CommitDetail, Repository};

pub fn export_file(
    repository: &Repository,
    git_ref: &str,
    path: &str,
    content: &str,
) -> Result<PathBuf> {
    let file_name = format!(
        "repotrek-export-{}-{}.html",
        sanitize_filename(&repository.full_name),
        sanitize_filename(path)
    );
    let output = std::env::current_dir()?.join(file_name);

    let rows = content
        .lines()
        .enumerate()
        .map(|(index, line)| {
            format!(
                "<tr><td class=\"line-number\">{}</td><td class=\"source\"><code>{}</code></td></tr>",
                index + 1,
                escape_html(line)
            )
        })
        .collect::<String>();

    let title = format!("{} · {path}", repository.full_name);
    let body = format!(
        r#"<header>
  <div class="eyebrow">RepoTrek source export</div>
  <h1>{}</h1>
  <div class="metadata">
    <span>Repository: <a href="{}">{}</a></span>
    <span>Branch: {}</span>
    <span>Exported: {}</span>
  </div>
</header>
<main>
  <table class="code-table"><tbody>{rows}</tbody></table>
</main>"#,
        escape_html(path),
        escape_html(&repository.html_url),
        escape_html(&repository.full_name),
        escape_html(git_ref),
        Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
    );

    write_html(&output, &title, &body)?;
    Ok(output)
}

pub fn export_commit(repository: &Repository, detail: &CommitDetail) -> Result<PathBuf> {
    let file_name = format!(
        "repotrek-export-{}-commit-{}.html",
        sanitize_filename(&repository.full_name),
        detail.summary.short_sha()
    );
    let output = std::env::current_dir()?.join(file_name);

    let files = detail
        .files
        .iter()
        .map(|file| {
            let patch = file.patch.as_deref().map_or_else(
                || {
                    "<p class=\"notice\">GitHub APIの応答にpatch本文が含まれていません。</p>"
                        .to_owned()
                },
                render_patch,
            );
            format!(
                r#"<section class="diff-file">
  <h2>{}</h2>
  <div class="file-stats">{} · +{} −{} · {} changes</div>
  <div class="diff">{patch}</div>
</section>"#,
                escape_html(&file.filename),
                escape_html(&file.status),
                file.additions,
                file.deletions,
                file.changes,
            )
        })
        .collect::<String>();

    let body_text = if detail.summary.body.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<pre class=\"commit-body\">{}</pre>",
            escape_html(&detail.summary.body)
        )
    };

    let body = format!(
        r#"<header>
  <div class="eyebrow">RepoTrek commit export</div>
  <h1>{}</h1>
  <div class="metadata">
    <span>Repository: <a href="{}">{}</a></span>
    <span>Commit: <a href="{}">{}</a></span>
    <span>Author: {}</span>
    <span>Exported: {}</span>
  </div>
</header>
<main>
  <section class="commit-summary">
    {body_text}
    <p><strong>{} files</strong> · <span class="add">+{}</span> · <span class="del">−{}</span> · {} total changes</p>
  </section>
  {files}
</main>"#,
        escape_html(&detail.summary.title),
        escape_html(&repository.html_url),
        escape_html(&repository.full_name),
        escape_html(&detail.summary.html_url),
        escape_html(&detail.summary.sha),
        escape_html(&detail.summary.author_name),
        Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
        detail.files.len(),
        detail.stats.additions,
        detail.stats.deletions,
        detail.stats.total,
    );

    let title = format!(
        "{} · commit {}",
        repository.full_name,
        detail.summary.short_sha()
    );
    write_html(&output, &title, &body)?;
    Ok(output)
}

fn render_patch(patch: &str) -> String {
    patch
        .lines()
        .map(|line| {
            let class = if line.starts_with("+++") || line.starts_with("---") {
                "diff-meta"
            } else if line.starts_with('+') {
                "diff-add"
            } else if line.starts_with('-') {
                "diff-del"
            } else if line.starts_with("@@") {
                "diff-hunk"
            } else {
                "diff-context"
            };
            format!("<span class=\"{class}\">{}</span>\n", escape_html(line))
        })
        .collect()
}

fn write_html(path: &Path, title: &str, body: &str) -> Result<()> {
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{}</title>
<style>
:root {{ color-scheme: light; --ink:#1f2328; --muted:#59636e; --line:#d0d7de; --soft:#f6f8fa; --add:#1a7f37; --add-bg:#dafbe1; --del:#cf222e; --del-bg:#ffebe9; --accent:#0969da; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; color:var(--ink); background:white; font:11pt/1.45 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; }}
header, main {{ max-width:1160px; margin:0 auto; padding:24px 32px; }}
header {{ border-bottom:1px solid var(--line); }}
.eyebrow {{ color:var(--accent); font-size:9pt; font-weight:700; letter-spacing:.08em; text-transform:uppercase; }}
h1 {{ margin:.25rem 0 .5rem; font-size:20pt; line-height:1.2; overflow-wrap:anywhere; }}
h2 {{ margin:0; font:600 11pt ui-monospace,SFMono-Regular,Menlo,monospace; overflow-wrap:anywhere; }}
.metadata {{ display:flex; flex-wrap:wrap; gap:.35rem 1rem; color:var(--muted); font-size:9pt; }}
.code-table {{ width:100%; border-collapse:collapse; table-layout:fixed; font:8.6pt/1.42 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; }}
.code-table tr {{ break-inside:avoid; }}
.line-number {{ width:4.7rem; padding:0 .8rem; color:var(--muted); background:var(--soft); border-right:1px solid var(--line); text-align:right; vertical-align:top; user-select:none; }}
.source {{ padding:0 .8rem; white-space:pre-wrap; overflow-wrap:anywhere; vertical-align:top; }}
.commit-summary {{ margin-bottom:1.5rem; padding:1rem 1.2rem; background:var(--soft); border:1px solid var(--line); border-radius:6px; }}
.commit-body {{ margin:.7rem 0; white-space:pre-wrap; font:9.3pt/1.5 ui-monospace,SFMono-Regular,Menlo,monospace; }}
.diff-file {{ margin:0 0 1.5rem; border:1px solid var(--line); border-radius:6px; overflow:hidden; break-inside:auto; }}
.diff-file h2, .file-stats {{ padding:.55rem .8rem; background:var(--soft); }}
.file-stats {{ padding-top:0; color:var(--muted); font-size:8.7pt; }}
.diff {{ margin:0; padding:.7rem 0; overflow-wrap:anywhere; white-space:pre-wrap; font:8.2pt/1.38 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; }}
.diff span {{ display:block; padding:0 .8rem; border-left:3px solid transparent; }}
.diff-add {{ background:var(--add-bg); border-left-color:var(--add)!important; }}
.diff-del {{ background:var(--del-bg); border-left-color:var(--del)!important; }}
.diff-hunk {{ color:#8250df; background:#fbefff; font-weight:600; }}
.diff-meta {{ color:var(--muted); font-weight:600; }}
.add {{ color:var(--add); }} .del {{ color:var(--del); }}
.notice {{ margin:.7rem .8rem; color:var(--muted); }}
@page {{ size:A4 portrait; margin:13mm 10mm 15mm; }}
@media print {{
  body {{ font-size:9pt; print-color-adjust:exact; -webkit-print-color-adjust:exact; }}
  header, main {{ max-width:none; padding:0; }}
  header {{ margin-bottom:5mm; }}
  a {{ color:inherit; text-decoration:none; }}
  .diff-file {{ break-before:auto; }}
  .diff-file h2 {{ break-after:avoid; }}
  .code-table {{ font-size:7.4pt; }}
  .line-number {{ width:12mm; }}
}}
</style>
</head>
<body>{body}</body>
</html>
"#,
        escape_html(title)
    );

    fs::write(path, html).with_context(|| format!("HTMLを書き出せません: {}", path.display()))
}

fn sanitize_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').to_owned()
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{escape_html, sanitize_filename};

    #[test]
    fn escapes_html() {
        assert_eq!(escape_html("<a&\"b\">"), "&lt;a&amp;&quot;b&quot;&gt;");
    }

    #[test]
    fn sanitizes_paths() {
        assert_eq!(
            sanitize_filename("yuna-r/repotrek/src/main.rs"),
            "yuna-r-repotrek-src-main.rs"
        );
    }
}
