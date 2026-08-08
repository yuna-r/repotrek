use chrono::{DateTime, Local, Utc};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{App, HomeFocus, Modal, RepositoryTab, Screen},
    highlight::source_spans,
    model::{CommitDetail, ContentKind, HistoryEntry, RepoCard},
};

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;
fn selection_style() -> Style {
    Style::new().bg(Color::DarkGray).fg(Color::White)
}

pub fn draw(frame: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);
    match app.screen {
        Screen::Home => draw_home(frame, body, app),
        Screen::Repository => draw_repository(frame, body, app),
        Screen::File => draw_file(frame, body, app),
        Screen::Commit => draw_commit(frame, body, app),
    }
    draw_footer(frame, footer, app);

    if let Some(modal) = &app.modal {
        draw_modal(frame, app, modal);
    }
    if let Some(message) = &app.loading {
        draw_loading(frame, message);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let context = match app.screen {
        Screen::Home => "Terminal-first source code browser".to_owned(),
        Screen::Repository => app.repository.as_ref().map_or_else(
            || "Repository".to_owned(),
            |state| {
                let branch = app
                    .icons
                    .label(app.icons.branch, &state.repository.default_branch);
                format!("{}  {branch}", state.repository.full_name)
            },
        ),
        Screen::File => app.repository.as_ref().map_or_else(
            || "File".to_owned(),
            |repository| {
                let path = app.file.as_ref().map_or("", |file| file.path.as_str());
                format!("{} / {path}", repository.repository.full_name)
            },
        ),
        Screen::Commit => app.repository.as_ref().map_or_else(
            || "Commit".to_owned(),
            |repository| {
                let sha = app
                    .commit
                    .as_ref()
                    .map_or("", |commit| commit.detail.summary.short_sha());
                format!("{} / commit / {sha}", repository.repository.full_name)
            },
        ),
    };

    let line = Line::from(vec![
        Span::styled(
            " RepoTrek ",
            Style::new()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(context, Style::new().add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_home(frame: &mut Frame, area: Rect, app: &App) {
    let [search_area, lists_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(area);

    let search_title = app.icons.label(app.icons.search, "Repository");
    let search_border = if app.home.focus == HomeFocus::Search {
        Style::new().fg(ACCENT)
    } else {
        Style::new().fg(MUTED)
    };
    let cursor = if app.home.focus == HomeFocus::Search {
        "▏"
    } else {
        ""
    };
    let query = if app.home.query.is_empty() {
        Line::from(vec![
            Span::styled("> ", Style::new().fg(ACCENT)),
            Span::styled(
                "owner/repo, GitHub URL, or git@github.com URL",
                Style::new().fg(MUTED),
            ),
            Span::raw(cursor),
        ])
    } else {
        Line::from(vec![
            Span::styled("> ", Style::new().fg(ACCENT)),
            Span::raw(app.home.query.clone()),
            Span::styled(cursor, Style::new().fg(ACCENT)),
        ])
    };
    frame.render_widget(
        Paragraph::new(query).block(
            Block::default()
                .borders(Borders::ALL)
                .title(search_title)
                .border_style(search_border),
        ),
        search_area,
    );

    if lists_area.width >= 105 {
        let [history, featured, recommended] = Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .areas(lists_area);
        draw_history(frame, history, app);
        draw_cards(
            frame,
            featured,
            app,
            HomeFocus::Featured,
            app.icons.label(app.icons.featured, "Featured"),
            &app.home.featured,
            app.home.featured_index,
        );
        draw_cards(
            frame,
            recommended,
            app,
            HomeFocus::Recommended,
            app.icons.label(app.icons.recommended, "Recommended"),
            &app.home.recommended,
            app.home.recommended_index,
        );
    } else {
        let [history, featured, recommended] = Layout::vertical([
            Constraint::Percentage(38),
            Constraint::Percentage(31),
            Constraint::Percentage(31),
        ])
        .areas(lists_area);
        draw_history(frame, history, app);
        draw_cards(
            frame,
            featured,
            app,
            HomeFocus::Featured,
            app.icons.label(app.icons.featured, "Featured"),
            &app.home.featured,
            app.home.featured_index,
        );
        draw_cards(
            frame,
            recommended,
            app,
            HomeFocus::Recommended,
            app.icons.label(app.icons.recommended, "Recommended"),
            &app.home.recommended,
            app.home.recommended_index,
        );
    }
}

fn draw_history(frame: &mut Frame, area: Rect, app: &App) {
    let title = app.icons.label(app.icons.history, "History");
    let focused = app.home.focus == HomeFocus::History;
    let inner_height = area.height.saturating_sub(2) as usize;
    let visible_count = (inner_height / 2).max(1);
    let (start, end) = window_bounds(
        app.home.history.len(),
        app.home.history_index,
        visible_count,
    );

    let items: Vec<ListItem<'static>> = app.home.history[start..end]
        .iter()
        .enumerate()
        .map(|(offset, entry)| {
            history_item(entry, start + offset == app.home.history_index, focused)
        })
        .collect();
    let items = if items.is_empty() {
        vec![ListItem::new(Line::styled(
            "No history yet",
            Style::new().fg(MUTED),
        ))]
    } else {
        items
    };

    frame.render_widget(List::new(items).block(section_block(&title, focused)), area);
}

fn history_item(entry: &HistoryEntry, selected: bool, focused: bool) -> ListItem<'static> {
    let marker = if selected && focused { "› " } else { "  " };
    let location = entry.last_path.as_deref().unwrap_or("repository root");
    let lines = vec![
        Line::from(vec![
            Span::styled(marker, Style::new().fg(ACCENT)),
            Span::styled(
                entry.repository.id.full_name(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(truncate(location, 32), Style::new().fg(MUTED)),
            Span::styled(
                format!(" · {}", relative_time(&entry.visited_at)),
                Style::new().fg(MUTED),
            ),
        ]),
    ];
    ListItem::new(lines).style(if selected && focused {
        selection_style()
    } else {
        Style::default()
    })
}

#[allow(clippy::too_many_arguments)]
fn draw_cards(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    focus: HomeFocus,
    title: String,
    cards: &[RepoCard],
    selected_index: usize,
) {
    let focused = app.home.focus == focus;
    let inner_height = area.height.saturating_sub(2) as usize;
    let visible_count = (inner_height / 2).max(1);
    let (start, end) = window_bounds(cards.len(), selected_index, visible_count);
    let items: Vec<ListItem<'static>> = cards[start..end]
        .iter()
        .enumerate()
        .map(|(offset, card)| card_item(card, start + offset == selected_index, focused))
        .collect();
    let items = if items.is_empty() {
        vec![ListItem::new(Line::styled(
            "No repositories",
            Style::new().fg(MUTED),
        ))]
    } else {
        items
    };
    frame.render_widget(List::new(items).block(section_block(&title, focused)), area);
}

fn card_item(card: &RepoCard, selected: bool, focused: bool) -> ListItem<'static> {
    let marker = if selected && focused { "› " } else { "  " };
    let language = card.language.as_deref().unwrap_or("Unknown");
    let stars = if card.stars == 0 {
        String::new()
    } else {
        format!(" · ★{}", compact_number(card.stars))
    };
    let description = card.description.as_deref().unwrap_or("No description");
    let updated = card
        .updated_at
        .as_ref()
        .map_or_else(String::new, |time| format!(" · {}", relative_time(time)));
    let lines = vec![
        Line::from(vec![
            Span::styled(marker, Style::new().fg(ACCENT)),
            Span::styled(
                card.id.full_name(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{language}{stars}{updated} · {}", truncate(description, 34)),
                Style::new().fg(MUTED),
            ),
        ]),
    ];
    ListItem::new(lines).style(if selected && focused {
        selection_style()
    } else {
        Style::default()
    })
}

fn draw_repository(frame: &mut Frame, area: Rect, app: &App) {
    let Some(state) = app.repository.as_ref() else {
        frame.render_widget(Paragraph::new("Repository state is unavailable"), area);
        return;
    };

    let [summary_area, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(1),
    ])
    .areas(area);

    let description = state
        .repository
        .description
        .as_deref()
        .unwrap_or("No description");
    let privacy = if state.repository.is_private {
        "Private"
    } else {
        "Public"
    };
    let summary = vec![
        Line::from(vec![
            Span::styled(
                state.repository.full_name.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {privacy}"), Style::new().fg(MUTED)),
        ]),
        Line::from(vec![
            Span::styled(truncate(description, 90), Style::new().fg(MUTED)),
            Span::raw("  "),
            Span::styled(
                format!(
                    "{} {}   {} {}",
                    app.icons.star,
                    compact_number(state.repository.stargazers_count),
                    app.icons.fork,
                    compact_number(state.repository.forks_count)
                ),
                Style::new().fg(MUTED),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(summary), summary_area);

    let titles = [
        "Code",
        "Commits",
        "Pull requests",
        "Issues",
        "Actions",
        "Releases",
    ]
    .into_iter()
    .map(Line::from)
    .collect::<Vec<_>>();
    let selected = match state.tab {
        RepositoryTab::Code => 0,
        RepositoryTab::Commits => 1,
    };
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .divider("  ")
            .highlight_style(
                Style::new()
                    .fg(ACCENT)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
            .block(Block::default().borders(Borders::BOTTOM)),
        tabs_area,
    );

    match state.tab {
        RepositoryTab::Code => draw_code(frame, content_area, app),
        RepositoryTab::Commits => draw_commits(frame, content_area, app),
    }
}

fn draw_code(frame: &mut Frame, area: Rect, app: &App) {
    let Some(state) = app.repository.as_ref() else {
        return;
    };
    let path = if state.path.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", state.path)
    };
    let title = format!("{}  {path}", state.repository.default_branch);
    let visible = area.height.saturating_sub(2) as usize;
    let (start, end) = window_bounds(state.entries.len(), state.entry_index, visible.max(1));
    let items: Vec<ListItem<'static>> = state.entries[start..end]
        .iter()
        .enumerate()
        .map(|(offset, entry)| {
            let selected = start + offset == state.entry_index;
            let icon = match entry.kind {
                ContentKind::Directory => app.icons.folder,
                _ => app.icons.file,
            };
            let suffix = if entry.kind.is_directory() { "/" } else { "" };
            let kind = match entry.kind {
                ContentKind::Directory => "directory".to_owned(),
                ContentKind::File | ContentKind::Symlink => format_size(entry.size),
                ContentKind::Submodule => "submodule".to_owned(),
                ContentKind::Unknown => "unknown".to_owned(),
            };
            let object = entry.sha.get(..7).unwrap_or(&entry.sha);
            let marker = if selected { "›" } else { " " };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{marker} "), Style::new().fg(ACCENT)),
                Span::raw(if icon.is_empty() {
                    String::new()
                } else {
                    format!("{icon} ")
                }),
                Span::styled(
                    format!("{}{suffix}", entry.name),
                    if entry.kind.is_directory() {
                        Style::new().fg(Color::LightBlue)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(format!("  {kind} · {object}"), Style::new().fg(MUTED)),
            ]))
            .style(if selected {
                selection_style()
            } else {
                Style::default()
            })
        })
        .collect();
    let items = if items.is_empty() {
        vec![ListItem::new(Line::styled(
            "This directory is empty",
            Style::new().fg(MUTED),
        ))]
    } else {
        items
    };
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn draw_commits(frame: &mut Frame, area: Rect, app: &App) {
    let Some(state) = app.repository.as_ref() else {
        return;
    };
    if area.width >= 92 {
        let [list_area, preview_area] =
            Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)])
                .areas(area);
        draw_commit_list(frame, list_area, app);
        draw_commit_preview(frame, preview_area, app);
    } else {
        draw_commit_list(frame, area, app);
    }

    if state.commits.is_empty() {
        frame.render_widget(
            Paragraph::new("Press r to load commits").alignment(Alignment::Center),
            area.inner(Margin {
                horizontal: 2,
                vertical: 2,
            }),
        );
    }
}

fn draw_commit_list(frame: &mut Frame, area: Rect, app: &App) {
    let Some(state) = app.repository.as_ref() else {
        return;
    };
    let visible_count = (area.height.saturating_sub(2) as usize / 2).max(1);
    let (start, end) = window_bounds(state.commits.len(), state.commit_index, visible_count);
    let items = state.commits[start..end]
        .iter()
        .enumerate()
        .map(|(offset, commit)| {
            let selected = start + offset == state.commit_index;
            let marker = if selected { "›" } else { " " };
            let verified = if commit.verified {
                format!(" {}", app.icons.verified)
            } else {
                String::new()
            };
            let date = commit
                .authored_at
                .as_ref()
                .map_or_else(|| "unknown date".to_owned(), relative_time);
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{marker} {} ", app.icons.commit),
                        Style::new().fg(ACCENT),
                    ),
                    Span::styled(
                        truncate(&commit.title, 54),
                        Style::new().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(verified, Style::new().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        format!("{} · {date} · {}", commit.author_name, commit.short_sha()),
                        Style::new().fg(MUTED),
                    ),
                ]),
            ])
            .style(if selected {
                selection_style()
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    let title = format!("Commits · page {}", state.commit_page);
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn draw_commit_preview(frame: &mut Frame, area: Rect, app: &App) {
    let Some(commit) = app
        .repository
        .as_ref()
        .and_then(|state| state.selected_commit())
    else {
        frame.render_widget(
            Paragraph::new("Select a commit").block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    };

    let date = commit.authored_at.as_ref().map_or_else(
        || "unknown date".to_owned(),
        |date| {
            date.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S %Z")
                .to_string()
        },
    );
    let verified = if commit.verified {
        format!("{} Verified", app.icons.verified)
    } else {
        "Unverified".to_owned()
    };
    let parent = commit
        .parent_shas
        .first()
        .map_or("none", |sha| sha.get(..7).unwrap_or(sha));
    let text = vec![
        Line::styled(
            commit.title.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Author: ", Style::new().fg(MUTED)),
            Span::raw(commit.author_name.clone()),
        ]),
        Line::from(vec![
            Span::styled("Date:   ", Style::new().fg(MUTED)),
            Span::raw(date),
        ]),
        Line::from(vec![
            Span::styled("Commit: ", Style::new().fg(MUTED)),
            Span::raw(commit.sha.clone()),
        ]),
        Line::from(vec![
            Span::styled("Parent: ", Style::new().fg(MUTED)),
            Span::raw(parent.to_owned()),
        ]),
        Line::from(vec![
            Span::styled("Sign:   ", Style::new().fg(MUTED)),
            Span::styled(
                verified,
                if commit.verified {
                    Style::new().fg(Color::Green)
                } else {
                    Style::new().fg(MUTED)
                },
            ),
        ]),
        Line::raw(""),
        Line::raw(commit.body.clone()),
        Line::raw(""),
        Line::styled("Enter: open full diff", Style::new().fg(ACCENT)),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Preview")),
        area,
    );
}

fn draw_file(frame: &mut Frame, area: Rect, app: &App) {
    let Some(file) = app.file.as_ref() else {
        frame.render_widget(Paragraph::new("File state is unavailable"), area);
        return;
    };
    let [tabs_area, content_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
    frame.render_widget(
        Tabs::new(["Code", "Blame", "History"])
            .select(0)
            .divider("  ")
            .highlight_style(
                Style::new()
                    .fg(ACCENT)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        tabs_area,
    );

    let total_lines = file.line_count();
    let block = Block::default().borders(Borders::ALL).title(format!(
        "{} · lines {}–{} / {}",
        file.path,
        file.vertical_scroll.saturating_add(1).min(total_lines),
        (file.vertical_scroll + content_area.height.saturating_sub(2) as usize).min(total_lines),
        total_lines
    ));
    let inner = block.inner(content_area);
    frame.render_widget(block, content_area);

    let number_width = total_lines.to_string().len().clamp(3, 8) as u16 + 2;
    let [numbers_area, source_area] =
        Layout::horizontal([Constraint::Length(number_width), Constraint::Min(1)]).areas(inner);
    let visible_count = inner.height as usize;
    let extension = file
        .path
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension);
    let visible_lines: Vec<&str> = file
        .content
        .lines()
        .skip(file.vertical_scroll)
        .take(visible_count)
        .collect();
    let numbers = visible_lines
        .iter()
        .enumerate()
        .map(|(offset, _)| {
            Line::styled(
                format!(
                    "{:>width$} │",
                    file.vertical_scroll + offset + 1,
                    width = number_width.saturating_sub(2) as usize
                ),
                Style::new().fg(MUTED),
            )
        })
        .collect::<Vec<_>>();
    let source = visible_lines
        .into_iter()
        .map(|line| Line::from(source_spans(line, extension)))
        .collect::<Vec<_>>();

    frame.render_widget(
        Paragraph::new(numbers).style(Style::new().bg(Color::Black)),
        numbers_area,
    );
    frame.render_widget(
        Paragraph::new(source).scroll((0, file.horizontal_scroll.min(u16::MAX as usize) as u16)),
        source_area,
    );
}

fn draw_commit(frame: &mut Frame, area: Rect, app: &App) {
    let Some(commit) = app.commit.as_ref() else {
        frame.render_widget(Paragraph::new("Commit state is unavailable"), area);
        return;
    };
    let lines = commit_detail_lines(&commit.detail, app);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((commit.vertical_scroll.min(u16::MAX as usize) as u16, 0))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(commit.detail.summary.title.clone()),
            ),
        area,
    );
}

fn commit_detail_lines(detail: &CommitDetail, app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::styled(
            detail.summary.title.clone(),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Line::from(vec![
            Span::styled("Author  ", Style::new().fg(MUTED)),
            Span::raw(detail.summary.author_name.clone()),
        ]),
        Line::from(vec![
            Span::styled("Commit  ", Style::new().fg(MUTED)),
            Span::raw(detail.summary.sha.clone()),
        ]),
        Line::from(vec![
            Span::styled("Stats   ", Style::new().fg(MUTED)),
            Span::styled(
                format!("+{}", detail.stats.additions),
                Style::new().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(
                format!("-{}", detail.stats.deletions),
                Style::new().fg(Color::Red),
            ),
            Span::styled(
                format!("  {} files", detail.files.len()),
                Style::new().fg(MUTED),
            ),
        ]),
    ];
    if detail.summary.verified {
        lines.push(Line::styled(
            format!("{} Verified signature", app.icons.verified),
            Style::new().fg(Color::Green),
        ));
    }
    if !detail.summary.body.trim().is_empty() {
        lines.push(Line::raw(""));
        lines.extend(
            detail
                .summary
                .body
                .lines()
                .map(|line| Line::raw(line.to_owned())),
        );
    }

    for file in &detail.files {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(
                "{}  +{} -{}  {}",
                file.filename, file.additions, file.deletions, file.status
            ),
            Style::new()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled("─".repeat(72), Style::new().fg(MUTED)));
        if let Some(patch) = &file.patch {
            for patch_line in patch.lines() {
                let style = if patch_line.starts_with("+++") || patch_line.starts_with("---") {
                    Style::new().fg(MUTED)
                } else if patch_line.starts_with('+') {
                    Style::new().fg(Color::Green).bg(Color::Rgb(0, 30, 10))
                } else if patch_line.starts_with('-') {
                    Style::new().fg(Color::Red).bg(Color::Rgb(35, 0, 0))
                } else if patch_line.starts_with("@@") {
                    Style::new().fg(Color::LightMagenta)
                } else {
                    Style::default()
                };
                lines.push(Line::styled(patch_line.to_owned(), style));
            }
        } else {
            lines.push(Line::styled(
                "Patch body is not present in the GitHub API response.",
                Style::new().fg(MUTED),
            ));
        }
    }
    lines
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let hints = match app.screen {
        Screen::Home if app.home.focus == HomeFocus::Search => {
            "Enter open   Tab sections   Ctrl+U clear   Esc clear"
        }
        Screen::Home => "j/k move   Enter open   / search   r refresh   ? help   q quit",
        Screen::Repository => {
            if app
                .repository
                .as_ref()
                .is_some_and(|repository| repository.tab == RepositoryTab::Commits)
            {
                "j/k move   Enter diff   n/p page   1/2 tabs   r refresh   Esc home"
            } else {
                "j/k move   Enter open   Backspace parent   1/2 tabs   r refresh   Esc home"
            }
        }
        Screen::File => "j/k scroll   h/l horizontal   g/G top/bottom   p print HTML   b back",
        Screen::Commit => "j/k scroll   PgUp/PgDn   p print HTML   b back   ? help",
    };

    let mut right = Vec::new();
    if let Some(status) = &app.status {
        right.push(status.clone());
    }
    if let Some(rate) = &app.rate_limit
        && rate.remaining.is_some_and(|remaining| remaining <= 5)
    {
        let limit = rate.limit.unwrap_or_default();
        let remaining = rate.remaining.unwrap_or_default();
        right.push(format!("API {remaining}/{limit}"));
    }
    if app.authenticated {
        right.push("authenticated".to_owned());
    }
    let right = right.join(" · ");
    let width = area.width as usize;
    let right_width = UnicodeWidthStr::width(right.as_str());
    let left_budget = width.saturating_sub(right_width + 3);
    let left = truncate(hints, left_budget);
    let left_width = UnicodeWidthStr::width(left.as_str());
    let spacer = width.saturating_sub(left_width + right_width);
    let line = Line::from(vec![
        Span::styled(left, Style::new().fg(MUTED)),
        Span::raw(" ".repeat(spacer)),
        Span::styled(right, Style::new().fg(MUTED)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_modal(frame: &mut Frame, app: &App, modal: &Modal) {
    let (title, lines, height) = match modal {
        Modal::Help => ("Help", help_lines(app), 19),
        Modal::Error { title, message } => (
            title.as_str(),
            std::iter::once(Line::from(vec![
                Span::styled(
                    format!("{} ", app.icons.warning),
                    Style::new().fg(Color::Yellow),
                ),
                Span::styled(title.clone(), Style::new().add_modifier(Modifier::BOLD)),
            ]))
            .chain(std::iter::once(Line::raw("")))
            .chain(message.lines().map(|line| Line::raw(line.to_owned())))
            .chain(std::iter::once(Line::raw("")))
            .chain(std::iter::once(Line::styled(
                "Enter / Esc: close",
                Style::new().fg(ACCENT),
            )))
            .collect(),
            9,
        ),
        Modal::RateLimit { rate_limit } => {
            let reset = rate_limit.reset_at().map_or_else(
                || "unknown".to_owned(),
                |time| {
                    time.with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M:%S %Z")
                        .to_string()
                },
            );
            let mut lines = vec![
                Line::raw("GitHub REST APIの利用上限に達しました。"),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("Reset: ", Style::new().fg(MUTED)),
                    Span::raw(reset),
                ]),
                Line::raw(""),
            ];
            if app.authenticated {
                lines.push(Line::raw(
                    "認証済みの上限です。Escでキャッシュ済みデータへ戻れます。",
                ));
            } else {
                lines.push(Line::raw(
                    "EnterでGitHub CLI認証を開始し、上限を引き上げます。",
                ));
                lines.push(Line::styled(
                    "Tokenはghが保管し、RepoTrekはメモリ内でのみ使用します。",
                    Style::new().fg(MUTED),
                ));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                if app.authenticated {
                    "Esc: cached mode"
                } else {
                    "Enter: authenticate   Esc: cached mode"
                },
                Style::new().fg(ACCENT),
            ));
            ("GitHub API rate limit", lines, 12)
        }
    };

    let area = centered_rect(72, height, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(ACCENT))
                .title(title),
        ),
        area,
    );
}

fn draw_loading(frame: &mut Frame, message: &str) {
    let area = centered_rect(50, 5, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("● ", Style::new().fg(ACCENT)),
            Span::raw(message.to_owned()),
        ]))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(ACCENT)),
        ),
        area,
    );
}

fn help_lines(app: &App) -> Vec<Line<'static>> {
    let emoji = if app.icons.enabled { "on" } else { "off" };
    vec![
        Line::styled("Global", Style::new().add_modifier(Modifier::BOLD)),
        Line::raw("  ?              Help"),
        Line::raw("  Ctrl+C / q     Quit"),
        Line::raw(""),
        Line::styled("Home", Style::new().add_modifier(Modifier::BOLD)),
        Line::raw("  owner/repo     Open a GitHub repository"),
        Line::raw("  Tab            Move between History / Featured / Recommended"),
        Line::raw("  r              Refresh recommendations from GitHub Search"),
        Line::raw(""),
        Line::styled("Repository", Style::new().add_modifier(Modifier::BOLD)),
        Line::raw("  1 / 2          Code / Commits"),
        Line::raw("  Enter          Open directory, file, or commit"),
        Line::raw("  Backspace      Parent directory"),
        Line::raw(""),
        Line::styled("Reader", Style::new().add_modifier(Modifier::BOLD)),
        Line::raw("  j/k, PgUp/PgDn Scroll"),
        Line::raw(format!(
            "  p              {} Export print-ready HTML",
            app.icons.print
        )),
        Line::raw("  b              Back"),
        Line::raw(""),
        Line::styled(
            format!("Emoji mode resolved: {emoji}"),
            Style::new().fg(MUTED),
        ),
    ]
}

fn section_block<'a>(title: &'a str, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if focused {
            Style::new().fg(ACCENT)
        } else {
            Style::new().fg(MUTED)
        })
}

fn centered_rect(percent_x: u16, height: u16, outer: Rect) -> Rect {
    let width = outer
        .width
        .saturating_mul(percent_x)
        .saturating_div(100)
        .max(20);
    let width = width.min(outer.width.saturating_sub(2).max(1));
    let height = height.min(outer.height.saturating_sub(2).max(1));
    Rect {
        x: outer.x + outer.width.saturating_sub(width) / 2,
        y: outer.y + outer.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn window_bounds(len: usize, selected: usize, visible: usize) -> (usize, usize) {
    if len == 0 {
        return (0, 0);
    }
    let visible = visible.max(1).min(len);
    let half = visible / 2;
    let start = selected
        .saturating_sub(half)
        .min(len.saturating_sub(visible));
    (start, start + visible)
}

fn relative_time(time: &DateTime<Utc>) -> String {
    let seconds = Utc::now().signed_duration_since(*time).num_seconds().max(0);
    match seconds {
        0..=59 => format!("{seconds}s ago"),
        60..=3_599 => format!("{}m ago", seconds / 60),
        3_600..=86_399 => format!("{}h ago", seconds / 3_600),
        86_400..=2_592_000 => format!("{}d ago", seconds / 86_400),
        _ => time.format("%Y-%m-%d").to_string(),
    }
}

fn compact_number(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}m", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn format_size(size: u64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MiB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.1} KiB", size as f64 / 1024.0)
    } else {
        format!("{size} B")
    }
}

fn truncate(value: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_owned();
    }

    let content_width = max_width.saturating_sub(1);
    let mut width = 0;
    let mut shortened = String::new();
    for character in value.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + character_width > content_width {
            break;
        }
        shortened.push(character);
        width += character_width;
    }
    shortened.push('~');
    shortened
}
