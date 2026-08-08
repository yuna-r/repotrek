use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::Local;

use crate::{
    diff::{DiffKind, parse_patch},
    highlight::source_html,
    model::{CommitDetail, Repository},
};

pub fn export_file(
    repository: &Repository,
    git_ref: &str,
    path: &str,
    content: &str,
) -> Result<PathBuf> {
    let filename = format!(
        "repotrek-{}-{}.html",
        sanitize_filename(&repository.full_name),
        sanitize_filename(path)
    );
    let output = std::env::current_dir()?.join(filename);
    let rows = content
        .lines()
        .enumerate()
        .map(|(index, line)| {
            format!(
                "<tr><td class=\"ln\">{}</td><td class=\"code\"><code>{}</code></td></tr>",
                index + 1,
                source_html(line, path)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let body = format!(
        r#"
<header class="doc-header">
  <div>
    <div class="repo">{repo}</div>
    <div class="path">{path}</div>
  </div>
  <div class="meta">branch <strong>{git_ref}</strong><br>{generated}</div>
</header>
<section class="summary">
  <span>{line_count} lines</span>
  <span>source snapshot</span>
</section>
<div class="code-frame"><table class="source-table"><thead><tr><th class="ln">Line</th><th>Source</th></tr></thead><tbody>{rows}</tbody></table></div>
"#,
        repo = escape_html(&repository.full_name),
        path = escape_html(path),
        git_ref = escape_html(git_ref),
        generated = Local::now().format("%Y-%m-%d %H:%M %Z"),
        line_count = content.lines().count(),
    );
    write_document(
        &output,
        &format!("{} · {}", repository.full_name, path),
        &body,
    )?;
    Ok(output)
}

pub fn export_commit(repository: &Repository, detail: &CommitDetail) -> Result<PathBuf> {
    let filename = format!(
        "repotrek-{}-commit-{}.html",
        sanitize_filename(&repository.full_name),
        sanitize_filename(detail.summary.short_sha())
    );
    let output = std::env::current_dir()?.join(filename);
    let mut files_html = String::new();

    for file in &detail.files {
        let diff_html = file.patch.as_deref().map_or_else(
            || "<p class=\"muted\">Diff omitted by GitHub API for this file.</p>".to_owned(),
            |patch| {
                let rows = parse_patch(patch)
                    .into_iter()
                    .map(|line| match line.kind {
                        DiffKind::Hunk => format!(
                            "<tr class=\"hunk\"><td colspan=\"4\"><code>{}</code></td></tr>",
                            escape_html(&line.text)
                        ),
                        DiffKind::Meta => format!(
                            "<tr class=\"meta-line\"><td colspan=\"4\"><code>{}</code></td></tr>",
                            escape_html(&line.text)
                        ),
                        kind => {
                            let old = line.old_line.map_or_else(String::new, |value| value.to_string());
                            let new = line.new_line.map_or_else(String::new, |value| value.to_string());
                            let (class, sign) = match kind {
                                DiffKind::Add => ("add", "+"),
                                DiffKind::Delete => ("del", "-"),
                                DiffKind::Context => ("ctx", " "),
                                DiffKind::Hunk | DiffKind::Meta => unreachable!(),
                            };
                            format!(
                                "<tr class=\"{class}\"><td class=\"ln old\">{old}</td><td class=\"ln new\">{new}</td><td class=\"sign\">{sign}</td><td class=\"code\"><code>{code}</code></td></tr>",
                                code = source_html(&line.text, &file.filename)
                            )
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("<div class=\"code-frame\"><table class=\"diff-table\"><thead><tr><th class=\"ln\">Old</th><th class=\"ln\">New</th><th class=\"sign\"></th><th>Source</th></tr></thead><tbody>{rows}</tbody></table></div>")
            },
        );
        files_html.push_str(&format!(
            r#"
<section class="file-section">
  <h2>{filename}</h2>
  <div class="file-stats"><span class="plus">+{additions}</span><span class="minus">-{deletions}</span><span>{status}</span></div>
  {diff_html}
</section>
"#,
            filename = escape_html(&file.filename),
            additions = file.additions,
            deletions = file.deletions,
            status = escape_html(&file.status),
        ));
    }

    let parents = if detail.summary.parent_shas.is_empty() {
        "none".to_owned()
    } else {
        detail
            .summary
            .parent_shas
            .iter()
            .map(|sha| escape_html(sha.get(..7).unwrap_or(sha)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let verified = if detail.summary.verified {
        "Verified"
    } else {
        "Unverified"
    };
    let body = format!(
        r#"
<header class="doc-header">
  <div>
    <div class="repo">{repo}</div>
    <div class="path">Commit {short_sha}</div>
  </div>
  <div class="meta">{generated}</div>
</header>
<article class="commit-card">
  <h1>{title}</h1>
  <dl>
    <dt>Author</dt><dd>{author}</dd>
    <dt>Commit</dt><dd><code>{sha}</code></dd>
    <dt>Parent</dt><dd><code>{parents}</code></dd>
    <dt>Signature</dt><dd>{verified}</dd>
    <dt>Changes</dt><dd><span class="plus">+{additions}</span> <span class="minus">-{deletions}</span> across {file_count} files</dd>
  </dl>
  <pre class="message">{message}</pre>
</article>
{files_html}
"#,
        repo = escape_html(&repository.full_name),
        short_sha = escape_html(detail.summary.short_sha()),
        generated = Local::now().format("%Y-%m-%d %H:%M %Z"),
        title = escape_html(&detail.summary.title),
        author = escape_html(&detail.summary.author_name),
        sha = escape_html(&detail.summary.sha),
        message = escape_html(&detail.summary.body),
        additions = detail.stats.additions,
        deletions = detail.stats.deletions,
        file_count = detail.files.len(),
    );
    write_document(
        &output,
        &format!("{} · {}", repository.full_name, detail.summary.short_sha()),
        &body,
    )?;
    Ok(output)
}

fn write_document(path: &Path, title: &str, body: &str) -> Result<()> {
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
:root {{ color-scheme: light; }}
* {{ box-sizing: border-box; }}
html {{ background: #f6f8fa; }}
body {{
  margin: 0 auto;
  max-width: 1440px;
  padding: 36px 42px 72px;
  color: #1f2328;
  background: #fff;
  font: 15px/1.58 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}}
.doc-header {{ display:flex; justify-content:space-between; gap:24px; align-items:flex-start; padding-bottom:18px; border-bottom:1px solid #d0d7de; margin-bottom:18px; }}
.repo {{ font-size:20px; font-weight:700; color:#0969da; }}
.path {{ margin-top:4px; font-size:16px; font-weight:600; overflow-wrap:anywhere; }}
.meta {{ color:#57606a; text-align:right; font-size:12px; white-space:nowrap; }}
.summary, .file-stats {{ display:flex; gap:18px; color:#57606a; margin:10px 0; }}
.commit-card {{ border:1px solid #d0d7de; border-radius:8px; padding:18px 22px; margin-bottom:24px; break-inside:avoid; }}
.commit-card h1 {{ font-size:20px; margin:0 0 14px; }}
dl {{ display:grid; grid-template-columns:90px 1fr; gap:5px 12px; margin:0; }} dt {{ color:#57606a; }} dd {{ margin:0; }}
.message {{ white-space:pre-wrap; font:13px/1.5 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; background:#f6f8fa; border-radius:6px; padding:12px; }}
.file-section {{ margin:26px 0 34px; break-before:auto; }}
.file-section h2 {{ font:600 15px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; margin:0; padding:10px 12px; background:#f6f8fa; border:1px solid #d0d7de; border-bottom:0; border-radius:6px 6px 0 0; overflow-wrap:anywhere; }}
.code-frame {{ width:100%; overflow-x:auto; border:1px solid #d0d7de; border-radius:6px; background:#fff; }}
.source-table, .diff-table {{ width:100%; border-collapse:collapse; table-layout:auto; }}
.source-table thead, .diff-table thead {{ display:table-header-group; }}
.source-table th, .diff-table th {{ padding:5px 10px; text-align:left; color:#57606a; background:#f6f8fa; border-bottom:1px solid #d0d7de; font:600 11px/1.35 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
.source-table td, .diff-table td {{ padding:0; vertical-align:top; border-bottom:1px solid #f0f1f2; }}
.ln {{ width:58px; min-width:58px; padding:0 10px !important; text-align:right !important; user-select:none; color:#6e7781; background:#f6f8fa; border-right:1px solid #d8dee4; font:12px/1.6 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}
.diff-table .ln {{ width:52px; min-width:52px; }}
.sign {{ width:28px; min-width:28px; text-align:center !important; color:#6e7781; font:12px/1.6 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}
.code {{ padding:0 12px !important; }}
.code code, .hunk code, .meta-line code {{ white-space:pre; tab-size:4; font:13px/1.68 ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace; }}
.hunk td {{ padding:4px 10px !important; color:#0550ae; background:#ddf4ff; border-top:1px solid #b6e3ff; border-bottom:1px solid #b6e3ff; }}
.meta-line td {{ padding:3px 10px !important; color:#6e7781; }}
.add td {{ background:#e6ffec; }} .del td {{ background:#ffebe9; }}
.plus {{ color:#1a7f37; font-weight:650; }} .minus {{ color:#cf222e; font-weight:650; }} .muted {{ color:#6e7781; }}
@media print {{
  @page {{ size: A4 landscape; margin: 12mm 10mm 14mm; }}
  html, body {{ background:#fff !important; }}
  body {{ max-width:none; padding:0; font-size:10pt; -webkit-print-color-adjust:exact; print-color-adjust:exact; }}
  .doc-header {{ margin-bottom:4mm; }}
  .repo {{ font-size:14pt; }} .path {{ font-size:11pt; }}
  .code-frame {{ overflow:visible; border-color:#aeb6bf; }}
  .source-table, .diff-table {{ font-size:9.2pt; }}
  .source-table th, .diff-table th {{ font-size:8pt; }}
  .code code, .hunk code, .meta-line code, .ln, .sign {{ font-size:9pt; line-height:1.5; }}
  .code code {{ white-space:pre-wrap; overflow-wrap:break-word; word-break:normal; }}
  .file-section {{ break-inside:auto; margin:5mm 0 7mm; }}
  .file-section h2 {{ break-after:avoid; }}
  thead {{ break-after:avoid; }}
  tr {{ break-inside:avoid; }}
  a {{ color:inherit; text-decoration:none; }}
}}
</style>
</head>
<body>{body}</body>
</html>"#,
        title = escape_html(title),
    );
    fs::write(path, html).with_context(|| format!("Could not write HTML: {}", path.display()))
}

fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::sanitize_filename;

    #[test]
    fn sanitizes_paths() {
        assert_eq!(
            sanitize_filename("yuna-r/repotrek/src/main.rs"),
            "yuna-r-repotrek-src-main.rs"
        );
    }
}
