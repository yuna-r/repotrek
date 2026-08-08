use chrono::Local;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};

use crate::{
    app::{App, CodeSearchMode, FileTab, HomeFocus, Modal, RepositoryTab, Screen},
    diff::{DiffKind, parse_patch},
    highlight::source_spans,
    model::{BlameRange, CommitDetail, HistoryEntry, RepoCard},
};

const ACCENT: Color = Color::Rgb(88, 166, 255);
const TEXT: Color = Color::Rgb(230, 237, 243);
const MUTED: Color = Color::Rgb(139, 148, 158);
const BORDER: Color = Color::Rgb(48, 54, 61);
const SELECT: Color = Color::Rgb(56, 66, 82);
const CURSOR: Color = Color::Rgb(33, 38, 45);
const GREEN: Color = Color::Rgb(63, 185, 80);
const RED: Color = Color::Rgb(248, 81, 73);

pub fn draw(frame: &mut Frame, app: &App) {
    let [header, content, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);
    match app.screen {
        Screen::Home => draw_home(frame, content, app),
        Screen::Repository => draw_repository(frame, content, app),
        Screen::File => draw_file(frame, content, app),
        Screen::Commit => draw_commit(frame, content, app),
    }
    draw_footer(frame, footer, app);

    if let Some(modal) = app.modal.as_ref() {
        draw_modal(frame, app, modal);
    }
    if let Some(loading) = app.loading.as_ref() {
        draw_loading(frame, loading);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let title = match app.screen {
        Screen::Home => "RepoTrek".to_owned(),
        Screen::Repository => app.repository.as_ref().map_or_else(
            || "RepoTrek".to_owned(),
            |state| {
                format!(
                    "{}  {} {}",
                    state.repository.full_name, app.icons.branch, state.selected_ref
                )
            },
        ),
        Screen::File => app.repository.as_ref().map_or_else(
            || "RepoTrek".to_owned(),
            |state| {
                let path = app.file.as_ref().map_or("", |file| file.path.as_str());
                format!(
                    "{} / {path}  {} {}",
                    state.repository.full_name, app.icons.branch, state.selected_ref
                )
            },
        ),
        Screen::Commit => app.repository.as_ref().map_or_else(
            || "Commit".to_owned(),
            |state| format!("{} / commit", state.repository.full_name),
        ),
    };

    let auth = if let Some(user) = app.auth_user.as_deref() {
        format!("GitHub @{}", user)
    } else if app.authenticated {
        "GitHub authenticated".to_owned()
    } else {
        "GitHub anonymous".to_owned()
    };
    let rate = app.rate_limit.as_ref().and_then(|rate| {
        let remaining = rate.remaining?;
        let limit = rate.limit?;
        let resource = rate.resource.as_deref().unwrap_or("api");
        Some(format!("{resource} {remaining}/{limit}"))
    });
    let right = rate.map_or(auth.clone(), |rate| format!("{auth} · API {rate}"));

    let [left, right_area] = Layout::horizontal([
        Constraint::Min(10),
        Constraint::Length((right.len() + 2) as u16),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " RepoTrek ",
                Style::new()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {title}"),
                Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
        ])),
        left,
    );
    frame.render_widget(
        Paragraph::new(right)
            .alignment(Alignment::Right)
            .style(Style::new().fg(MUTED)),
        right_area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let hint = match app.screen {
        Screen::Home => "Enter open/search  Tab sections  r refresh  Ctrl+A auth  ? help  q quit",
        Screen::Repository => {
            "←/→ tabs  j/k move  Enter open  u up  B branch  f files  s code search  a auth  ? help"
        }
        Screen::File => {
            "Tab Code/Blame/History  j/k move  v select  y copy  @ symbols  d definition  p print  b back"
        }
        Screen::Commit => "j/k scroll  y copy SHA  p print  b back  ? help",
    };
    let text = app.status_text().unwrap_or(hint);
    let style = if app.status_text().is_some() {
        Style::new().fg(GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(MUTED)
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn draw_home(frame: &mut Frame, area: Rect, app: &App) {
    let [search_area, lists_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1)]).areas(area);
    let focused = app.home.focus == HomeFocus::Search;
    let title = app.icons.label(app.icons.search, "Repository or search");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(if focused { ACCENT } else { BORDER }))
        .title(title);
    let placeholder = if app.home.query.is_empty() {
        "owner/repo, GitHub URL, or search terms"
    } else {
        app.home.query.as_str()
    };
    frame.render_widget(
        Paragraph::new(format!("> {placeholder}"))
            .style(Style::new().fg(if app.home.query.is_empty() {
                MUTED
            } else {
                TEXT
            }))
            .block(block),
        search_area,
    );

    if lists_area.width >= 96 {
        let [history, featured, recommended] = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(lists_area);
        draw_history_list(frame, history, app);
        draw_card_list(
            frame,
            featured,
            app,
            HomeFocus::Featured,
            "Featured",
            app.icons.featured,
        );
        draw_card_list(
            frame,
            recommended,
            app,
            HomeFocus::Recommended,
            "Recommended",
            app.icons.recommended,
        );
    } else {
        let [history, featured, recommended] = Layout::vertical([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(lists_area);
        draw_history_list(frame, history, app);
        draw_card_list(
            frame,
            featured,
            app,
            HomeFocus::Featured,
            "Featured",
            app.icons.featured,
        );
        draw_card_list(
            frame,
            recommended,
            app,
            HomeFocus::Recommended,
            "Recommended",
            app.icons.recommended,
        );
    }
}

fn draw_history_list(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.home.focus == HomeFocus::History;
    let title = app.icons.label(app.icons.history, "History");
    let items = app
        .home
        .history
        .iter()
        .enumerate()
        .map(|(index, entry)| history_item(entry, index == app.home.history_index, focused))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new("No history yet")]
        } else {
            items
        })
        .block(section_block(&title, focused)),
        area,
    );
}

fn history_item(entry: &HistoryEntry, selected: bool, focused: bool) -> ListItem<'static> {
    let location = entry.last_path.as_deref().unwrap_or("/");
    let time = entry.visited_at.with_timezone(&Local).format("%m-%d %H:%M");
    ListItem::new(vec![
        Line::styled(
            entry.repository.id.full_name(),
            Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Line::styled(format!("  {location} · {time}"), Style::new().fg(MUTED)),
    ])
    .style(item_style(selected, focused))
}

fn draw_card_list(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    focus: HomeFocus,
    title: &str,
    icon: &str,
) {
    let focused = app.home.focus == focus;
    let (cards, selected) = match focus {
        HomeFocus::Featured => (&app.home.featured, app.home.featured_index),
        HomeFocus::Recommended => (&app.home.recommended, app.home.recommended_index),
        _ => return,
    };
    let title = app.icons.label(icon, title);
    let items = cards
        .iter()
        .enumerate()
        .map(|(index, card)| card_item(card, index == selected, focused, app))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new("No results")]
        } else {
            items
        })
        .block(section_block(&title, focused)),
        area,
    );
}

fn card_item(card: &RepoCard, selected: bool, focused: bool, app: &App) -> ListItem<'static> {
    let language = card.language.as_deref().unwrap_or("-");
    let stars = if card.stars == 0 {
        String::new()
    } else {
        format!(" {} {}", app.icons.star, card.stars)
    };
    ListItem::new(vec![
        Line::styled(
            card.id.full_name(),
            Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Line::styled(format!("  {language}{stars}"), Style::new().fg(MUTED)),
    ])
    .style(item_style(selected, focused))
}

fn section_block(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(if focused { ACCENT } else { BORDER }))
        .title(title)
}

fn item_style(selected: bool, focused: bool) -> Style {
    if selected && focused {
        Style::new().bg(SELECT).fg(TEXT)
    } else if selected {
        Style::new().bg(CURSOR).fg(TEXT)
    } else {
        Style::default()
    }
}

fn draw_repository(frame: &mut Frame, area: Rect, app: &App) {
    let Some(state) = app.repository.as_ref() else {
        frame.render_widget(Paragraph::new("Repository state is unavailable"), area);
        return;
    };
    let [meta_area, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .areas(area);

    let description = state
        .repository
        .description
        .as_deref()
        .unwrap_or("No description");
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    state.repository.full_name.clone(),
                    Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "   {} {}   {} {}",
                        app.icons.star,
                        state.repository.stargazers_count,
                        app.icons.fork,
                        state.repository.forks_count
                    ),
                    Style::new().fg(MUTED),
                ),
            ]),
            Line::styled(description.to_owned(), Style::new().fg(MUTED)),
        ])
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::new().fg(BORDER)),
        ),
        meta_area,
    );

    let tabs = [
        "Code",
        "Commits",
        "Pull requests",
        "Issues",
        "Actions",
        "Releases",
    ];
    frame.render_widget(
        Tabs::new(tabs)
            .select(state.tab.index())
            .divider("  ")
            .highlight_style(
                Style::new()
                    .fg(ACCENT)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        tabs_area,
    );

    match state.tab {
        RepositoryTab::Code => draw_code_tree(frame, content_area, app),
        RepositoryTab::Commits => draw_commits(frame, content_area, app),
        RepositoryTab::PullRequests => draw_pull_requests(frame, content_area, app),
        RepositoryTab::Issues => draw_issues(frame, content_area, app),
        RepositoryTab::Actions => draw_actions(frame, content_area, app),
        RepositoryTab::Releases => draw_releases(frame, content_area, app),
    }
}

fn draw_code_tree(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.repository.as_ref().expect("repository state");
    let path = if state.path.is_empty() {
        "/"
    } else {
        state.path.as_str()
    };
    let visible_height = area.height.saturating_sub(2) as usize;
    let start = centered_window_start(state.entry_index, state.entries.len(), visible_height);
    let items = state
        .entries
        .iter()
        .skip(start)
        .take(visible_height)
        .enumerate()
        .map(|(offset, entry)| {
            let icon = if entry.kind.is_directory() {
                app.icons.folder
            } else {
                app.icons.file
            };
            let size = if entry.kind.is_file() {
                human_size(entry.size)
            } else {
                String::new()
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{icon} ")),
                Span::styled(
                    entry.name.clone(),
                    Style::new().fg(if entry.kind.is_directory() {
                        ACCENT
                    } else {
                        TEXT
                    }),
                ),
                Span::styled(format!("  {size}"), Style::new().fg(MUTED)),
            ]))
            .style(if start + offset == state.entry_index {
                Style::new().bg(SELECT)
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new("Empty directory")]
        } else {
            items
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{} {path}", app.icons.branch)),
        ),
        area,
    );
}

fn draw_commits(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.repository.as_ref().expect("repository state");
    if area.width >= 96 {
        let [list, preview] =
            Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)])
                .areas(area);
        draw_commit_list(frame, list, app);
        draw_commit_preview(frame, preview, app);
    } else {
        draw_commit_list(frame, area, app);
    }
    if state.commits.is_empty() {
        frame.render_widget(Paragraph::new("Loading or no commits"), area);
    }
}

fn draw_commit_list(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.repository.as_ref().expect("repository state");
    let visible = area.height.saturating_sub(2) as usize;
    let start = centered_window_start(state.commit_index, state.commits.len(), visible);
    let items = state
        .commits
        .iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, commit)| {
            let date = commit.authored_at.as_ref().map_or_else(
                || "unknown".to_owned(),
                |date| date.with_timezone(&Local).format("%m-%d %H:%M").to_string(),
            );
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("{} ", app.icons.commit), Style::new().fg(ACCENT)),
                    Span::styled(
                        commit.title.clone(),
                        Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::styled(
                    format!(
                        "    {} · {date} · {}",
                        commit.author_name,
                        commit.short_sha()
                    ),
                    Style::new().fg(MUTED),
                ),
            ])
            .style(if start + offset == state.commit_index {
                Style::new().bg(SELECT)
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Commits · page {}", state.commit_page)),
        ),
        area,
    );
}

fn draw_commit_preview(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.repository.as_ref().expect("repository state");
    let Some(commit) = state.selected_commit() else {
        frame.render_widget(Paragraph::new("Select a commit"), area);
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
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                commit.title.clone(),
                Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            Line::styled(
                format!("{} · {date}", commit.author_name),
                Style::new().fg(MUTED),
            ),
            Line::styled(
                format!("{} · {verified}", commit.sha),
                Style::new().fg(MUTED),
            ),
            Line::raw(""),
            Line::raw(commit.body.clone()),
        ])
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title("Preview")),
        area,
    );
}

fn draw_pull_requests(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.repository.as_ref().expect("repository state");
    let items = state
        .pull_requests
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let draft = if item.draft { " · Draft" } else { "" };
            ListItem::new(vec![
                Line::styled(
                    format!("#{}  {}", item.number, item.title),
                    Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    format!(
                        "  {} · {} → {} · {} comments{draft}",
                        item.author, item.head, item.base, item.comments
                    ),
                    Style::new().fg(MUTED),
                ),
            ])
            .style(if index == state.list_index {
                Style::new().bg(SELECT)
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    draw_generic_list(
        frame,
        area,
        "Open pull requests · Enter opens GitHub",
        items,
    );
}

fn draw_issues(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.repository.as_ref().expect("repository state");
    let items = state
        .issues
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let labels = if item.labels.is_empty() {
                String::new()
            } else {
                format!(" · {}", item.labels.join(", "))
            };
            ListItem::new(vec![
                Line::styled(
                    format!("#{}  {}", item.number, item.title),
                    Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    format!("  {} · {} comments{labels}", item.author, item.comments),
                    Style::new().fg(MUTED),
                ),
            ])
            .style(if index == state.list_index {
                Style::new().bg(SELECT)
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    draw_generic_list(frame, area, "Open issues · Enter opens GitHub", items);
}

fn draw_actions(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.repository.as_ref().expect("repository state");
    let items = state
        .workflow_runs
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let result = item.conclusion.as_deref().unwrap_or(&item.status);
            let marker = if result == "success" {
                "✓"
            } else if result == "failure" {
                "✗"
            } else {
                "●"
            };
            let color = if result == "success" {
                GREEN
            } else if result == "failure" {
                RED
            } else {
                ACCENT
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("{marker} "), Style::new().fg(color)),
                    Span::styled(
                        item.name.clone(),
                        Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::styled(
                    format!(
                        "  {} · {} · {}",
                        item.event,
                        item.branch,
                        item.created_at.with_timezone(&Local).format("%m-%d %H:%M")
                    ),
                    Style::new().fg(MUTED),
                ),
            ])
            .style(if index == state.list_index {
                Style::new().bg(SELECT)
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    draw_generic_list(frame, area, "Workflow runs · Enter opens GitHub", items);
}

fn draw_releases(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.repository.as_ref().expect("repository state");
    let items = state
        .releases
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let name = item.name.as_deref().unwrap_or(&item.tag_name);
            let flags = match (item.draft, item.prerelease) {
                (true, _) => "draft",
                (_, true) => "prerelease",
                _ => "release",
            };
            let date = item.published_at.as_ref().map_or_else(
                || "unpublished".to_owned(),
                |date| date.with_timezone(&Local).format("%Y-%m-%d").to_string(),
            );
            ListItem::new(vec![
                Line::styled(
                    format!("{}  {}", item.tag_name, name),
                    Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Line::styled(format!("  {flags} · {date}"), Style::new().fg(MUTED)),
            ])
            .style(if index == state.list_index {
                Style::new().bg(SELECT)
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    draw_generic_list(frame, area, "Releases · Enter opens GitHub", items);
}

fn draw_generic_list(frame: &mut Frame, area: Rect, title: &str, items: Vec<ListItem<'static>>) {
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new("No items")]
        } else {
            items
        })
        .block(Block::default().borders(Borders::ALL).title(title)),
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
    let selected = match file.tab {
        FileTab::Code => 0,
        FileTab::Blame => 1,
        FileTab::History => 2,
    };
    frame.render_widget(
        Tabs::new(["Code", "Blame", "History"])
            .select(selected)
            .divider("  ")
            .highlight_style(
                Style::new()
                    .fg(ACCENT)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        tabs_area,
    );
    match file.tab {
        FileTab::Code => draw_source_code(frame, content_area, app, false),
        FileTab::Blame => draw_source_code(frame, content_area, app, true),
        FileTab::History => draw_file_history(frame, content_area, app),
    }
}

fn draw_source_code(frame: &mut Frame, area: Rect, app: &App, blame: bool) {
    let file = app.file.as_ref().expect("file state");
    let total = file.line_count();
    let block_title = if file.selection_anchor.is_some() {
        let (start, end) = file.selection_range();
        format!("{} · lines {}-{} selected", file.path, start + 1, end + 1)
    } else {
        format!("{} · line {} / {}", file.path, file.cursor_line + 1, total)
    };
    let block = Block::default().borders(Borders::ALL).title(block_title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let visible = inner.height as usize;
    let mut start = file.viewport_top.min(total.saturating_sub(1));
    if file.cursor_line < start {
        start = file.cursor_line;
    }
    if file.cursor_line >= start + visible {
        start = file.cursor_line.saturating_sub(visible.saturating_sub(1));
    }
    let extension = file.path.rsplit_once('.').map_or("", |(_, ext)| ext);
    let (selection_start, selection_end) = file.selection_range();

    if blame {
        let blame_width = inner.width.min(38);
        let [blame_area, source_area] =
            Layout::horizontal([Constraint::Length(blame_width), Constraint::Min(1)]).areas(inner);
        let blame_lines = (start..start + visible)
            .take_while(|line| *line < total)
            .map(|line| {
                let range = blame_for_line(&file.blame, line + 1);
                let (sha, author) = range.map_or(("-------", "unknown"), |range| {
                    (range.commit_short_sha.as_str(), range.author.as_str())
                });
                let style = line_background(file, line, selection_start, selection_end);
                Line::styled(
                    format!("{sha:<8} {author:<18.18} {:>5} │", line + 1),
                    Style::new().fg(MUTED).bg(style),
                )
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(blame_lines), blame_area);
        let source = source_lines(
            file,
            start,
            visible,
            extension,
            selection_start,
            selection_end,
        );
        frame.render_widget(
            Paragraph::new(source)
                .scroll((0, file.horizontal_scroll.min(u16::MAX as usize) as u16)),
            source_area,
        );
    } else {
        let number_width = total.to_string().len().clamp(3, 8) as u16 + 3;
        let [numbers_area, source_area] =
            Layout::horizontal([Constraint::Length(number_width), Constraint::Min(1)]).areas(inner);
        let numbers = (start..start + visible)
            .take_while(|line| *line < total)
            .map(|line| {
                let bg = line_background(file, line, selection_start, selection_end);
                Line::styled(
                    format!(
                        "{:>width$} │",
                        line + 1,
                        width = number_width.saturating_sub(2) as usize
                    ),
                    Style::new().fg(MUTED).bg(bg),
                )
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(numbers), numbers_area);
        let source = source_lines(
            file,
            start,
            visible,
            extension,
            selection_start,
            selection_end,
        );
        frame.render_widget(
            Paragraph::new(source)
                .scroll((0, file.horizontal_scroll.min(u16::MAX as usize) as u16)),
            source_area,
        );
    }
}

fn source_lines(
    file: &crate::app::FileState,
    start: usize,
    visible: usize,
    extension: &str,
    selection_start: usize,
    selection_end: usize,
) -> Vec<Line<'static>> {
    file.content
        .lines()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, line)| {
            let line_index = start + offset;
            let bg = line_background(file, line_index, selection_start, selection_end);
            let spans = source_spans(line, extension)
                .into_iter()
                .map(|mut span| {
                    span.style = span.style.patch(Style::new().bg(bg));
                    span
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

fn line_background(
    file: &crate::app::FileState,
    line: usize,
    selection_start: usize,
    selection_end: usize,
) -> Color {
    if file.selection_anchor.is_some() && line >= selection_start && line <= selection_end {
        SELECT
    } else if line == file.cursor_line {
        CURSOR
    } else {
        Color::Reset
    }
}

fn blame_for_line(ranges: &[BlameRange], line: usize) -> Option<&BlameRange> {
    ranges
        .iter()
        .find(|range| line >= range.starting_line && line <= range.ending_line)
}

fn draw_file_history(frame: &mut Frame, area: Rect, app: &App) {
    let file = app.file.as_ref().expect("file state");
    let items = file
        .history
        .iter()
        .enumerate()
        .map(|(index, commit)| {
            let date = commit.authored_at.as_ref().map_or_else(
                || "unknown".to_owned(),
                |date| date.with_timezone(&Local).format("%Y-%m-%d").to_string(),
            );
            ListItem::new(vec![
                Line::styled(
                    format!("{}  {}", commit.short_sha(), commit.title),
                    Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    format!("  {} · {date}", commit.author_name),
                    Style::new().fg(MUTED),
                ),
            ])
            .style(if index == file.history_index {
                Style::new().bg(SELECT)
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new("No history loaded")]
        } else {
            items
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("History · {}", file.path)),
        ),
        area,
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
            Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Line::from(vec![
            Span::styled("Author  ", Style::new().fg(MUTED)),
            Span::raw(detail.summary.author_name.clone()),
        ]),
        Line::from(vec![
            Span::styled("Commit  ", Style::new().fg(MUTED)),
            Span::styled(detail.summary.sha.clone(), Style::new().fg(ACCENT)),
        ]),
        Line::from(vec![
            Span::styled("Stats   ", Style::new().fg(MUTED)),
            Span::styled(
                format!("+{}", detail.stats.additions),
                Style::new().fg(GREEN),
            ),
            Span::raw("  "),
            Span::styled(format!("-{}", detail.stats.deletions), Style::new().fg(RED)),
        ]),
        Line::from(vec![
            Span::styled("Verify  ", Style::new().fg(MUTED)),
            Span::raw(if detail.summary.verified {
                format!("{} Verified", app.icons.verified)
            } else {
                "Unverified".to_owned()
            }),
        ]),
    ];
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
                "{}  {}   +{} -{}",
                file.status, file.filename, file.additions, file.deletions
            ),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        if let Some(patch) = file.patch.as_deref() {
            let extension = file.filename.rsplit_once('.').map_or("", |(_, ext)| ext);
            for diff in parse_patch(patch) {
                match diff.kind {
                    DiffKind::Hunk => lines.push(Line::styled(
                        diff.text,
                        Style::new().fg(ACCENT).bg(Color::Rgb(22, 27, 34)),
                    )),
                    DiffKind::Meta => lines.push(Line::styled(diff.text, Style::new().fg(MUTED))),
                    kind => {
                        let old = diff
                            .old_line
                            .map_or_else(|| "     ".to_owned(), |line| format!("{line:>5}"));
                        let new = diff
                            .new_line
                            .map_or_else(|| "     ".to_owned(), |line| format!("{line:>5}"));
                        let (sign, bg) = match kind {
                            DiffKind::Add => ("+", Color::Rgb(3, 48, 20)),
                            DiffKind::Delete => ("-", Color::Rgb(64, 18, 24)),
                            _ => (" ", Color::Reset),
                        };
                        let mut spans = vec![Span::styled(
                            format!("{old} {new} {sign} "),
                            Style::new().fg(MUTED).bg(bg),
                        )];
                        spans.extend(source_spans(&diff.text, extension).into_iter().map(
                            |mut span| {
                                span.style = span.style.patch(Style::new().bg(bg));
                                span
                            },
                        ));
                        lines.push(Line::from(spans));
                    }
                }
            }
        } else {
            lines.push(Line::styled(
                "  Diff omitted by GitHub API for this file",
                Style::new().fg(MUTED),
            ));
        }
    }
    lines
}

fn draw_modal(frame: &mut Frame, app: &App, modal: &Modal) {
    match modal {
        Modal::Help => draw_text_modal(frame, "Help", help_lines(app), 82, 28),
        Modal::Error { title, message } => {
            let mut lines = message
                .lines()
                .map(|line| Line::raw(line.to_owned()))
                .collect::<Vec<_>>();
            lines.push(Line::raw(""));
            lines.push(Line::styled("Enter / Esc close", Style::new().fg(ACCENT)));
            draw_text_modal(frame, title, lines, 76, 12);
        }
        Modal::RateLimit { rate_limit } => {
            let reset = rate_limit.reset_at().map_or_else(
                || "unknown".to_owned(),
                |time| time.with_timezone(&Local).format("%H:%M:%S").to_string(),
            );
            draw_text_modal(
                frame,
                "GitHub API quota",
                vec![
                    Line::raw("Anonymous GitHub API quota is exhausted."),
                    Line::raw(format!("Reset: {reset}")),
                    Line::raw(""),
                    Line::styled("Enter / a  Authentication options", Style::new().fg(ACCENT)),
                    Line::styled(
                        "Esc        Continue with loaded data",
                        Style::new().fg(MUTED),
                    ),
                ],
                68,
                10,
            );
        }
        Modal::AuthMenu { index } => {
            let options = [
                "1  Sign in with GitHub CLI / browser",
                "2  Paste Personal Access Token for this session",
                "3  Paste token and save in macOS Keychain",
            ];
            let lines = options
                .iter()
                .enumerate()
                .map(|(i, option)| {
                    Line::styled(
                        (*option).to_owned(),
                        if i == *index {
                            Style::new().bg(SELECT).fg(TEXT)
                        } else {
                            Style::new().fg(TEXT)
                        },
                    )
                })
                .chain(std::iter::once(Line::raw("")))
                .chain(std::iter::once(Line::styled(
                    "Token text is never written to RepoTrek history or config files.",
                    Style::new().fg(MUTED),
                )))
                .collect();
            draw_text_modal(frame, "GitHub authentication", lines, 78, 12);
        }
        Modal::TokenInput { input, persist } => {
            let masked = "*".repeat(input.chars().count().min(72));
            let mode = if *persist {
                "macOS Keychain"
            } else {
                "session only"
            };
            draw_text_modal(
                frame,
                "GitHub token",
                vec![
                    Line::styled(format!("Store: {mode}"), Style::new().fg(MUTED)),
                    Line::raw(""),
                    Line::styled(format!("> {masked}"), Style::new().fg(TEXT)),
                    Line::raw(""),
                    Line::styled(
                        "Type or Ctrl+V paste · Enter connect · Esc cancel",
                        Style::new().fg(ACCENT),
                    ),
                ],
                82,
                10,
            );
        }
        Modal::BranchPicker {
            query,
            branches,
            index,
        } => {
            let filtered = branches
                .iter()
                .filter(|branch| {
                    query.is_empty()
                        || branch
                            .name
                            .to_ascii_lowercase()
                            .contains(&query.to_ascii_lowercase())
                })
                .collect::<Vec<_>>();
            let lines = filtered
                .iter()
                .take(20)
                .enumerate()
                .map(|(i, branch)| {
                    let protected = if branch.protected { " [protected]" } else { "" };
                    Line::styled(
                        format!("{}{}", branch.name, protected),
                        if i == *index {
                            Style::new().bg(SELECT).fg(TEXT)
                        } else {
                            Style::new().fg(TEXT)
                        },
                    )
                })
                .collect::<Vec<_>>();
            draw_palette(frame, "Switch branch", query, lines, 82, 26);
        }
        Modal::RepositorySearch {
            query,
            results,
            index,
        } => {
            let lines = results
                .iter()
                .take(20)
                .enumerate()
                .map(|(i, repo)| {
                    Line::styled(
                        format!(
                            "{:<40}  ★ {:>7}  {}",
                            repo.id.full_name(),
                            repo.stars,
                            repo.language.as_deref().unwrap_or("-")
                        ),
                        if i == *index {
                            Style::new().bg(SELECT).fg(TEXT)
                        } else {
                            Style::new().fg(TEXT)
                        },
                    )
                })
                .collect();
            draw_palette(frame, "Search repositories", query, lines, 90, 26);
        }
        Modal::FileSearch {
            query,
            results,
            index,
            ..
        } => {
            let lines = results
                .iter()
                .take(22)
                .enumerate()
                .map(|(i, path)| {
                    Line::styled(
                        path.clone(),
                        if i == *index {
                            Style::new().bg(SELECT).fg(TEXT)
                        } else {
                            Style::new().fg(TEXT)
                        },
                    )
                })
                .collect();
            draw_palette(frame, "Go to file", query, lines, 92, 28);
        }
        Modal::CodeSearch {
            mode,
            query,
            results,
            index,
        } => {
            let title = if *mode == CodeSearchMode::Definition {
                "Find definition / symbol"
            } else {
                "Search code in repository"
            };
            let lines = results
                .iter()
                .take(22)
                .enumerate()
                .map(|(i, result)| {
                    Line::styled(
                        result.path.clone(),
                        if i == *index {
                            Style::new().bg(SELECT).fg(TEXT)
                        } else {
                            Style::new().fg(TEXT)
                        },
                    )
                })
                .collect();
            draw_palette(frame, title, query, lines, 92, 28);
        }
        Modal::SymbolPicker {
            query,
            results,
            index,
            ..
        } => {
            let lines = results
                .iter()
                .take(22)
                .enumerate()
                .map(|(i, symbol)| {
                    Line::styled(
                        format!("{:>5}  {:<10} {}", symbol.line, symbol.kind, symbol.name),
                        if i == *index {
                            Style::new().bg(SELECT).fg(TEXT)
                        } else {
                            Style::new().fg(TEXT)
                        },
                    )
                })
                .collect();
            draw_palette(frame, "Symbols in file", query, lines, 78, 28);
        }
    }
}

fn draw_palette(
    frame: &mut Frame,
    title: &str,
    query: &str,
    mut lines: Vec<Line<'static>>,
    width_percent: u16,
    height: u16,
) {
    let mut content = vec![
        Line::styled(
            format!("> {query}"),
            Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            "Enter search/open · type to refine · Ctrl+V paste · Esc close",
            Style::new().fg(MUTED),
        ),
        Line::raw(""),
    ];
    if lines.is_empty() {
        lines.push(Line::styled("No results yet", Style::new().fg(MUTED)));
    }
    content.extend(lines);
    draw_text_modal(frame, title, content, width_percent, height);
}

fn draw_text_modal(
    frame: &mut Frame,
    title: &str,
    lines: Vec<Line<'static>>,
    width_percent: u16,
    height: u16,
) {
    let area = centered_rect(width_percent, height, frame.area());
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
    let area = centered_rect(56, 5, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!("{}  {message}", "●"))
            .alignment(Alignment::Center)
            .style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(ACCENT)),
            ),
        area,
    );
}

fn help_lines(app: &App) -> Vec<Line<'static>> {
    vec![
        Line::styled("Repository", Style::new().add_modifier(Modifier::BOLD)),
        Line::raw(
            "  ←/→ or h/l      Move across Code, Commits, Pull requests, Issues, Actions, Releases",
        ),
        Line::raw("  1..6             Open a repository tab"),
        Line::raw("  u                 Go to parent directory"),
        Line::raw("  B                 Switch branch"),
        Line::raw("  f                 Go to file (recursive tree, cached)"),
        Line::raw("  s or /            Full-text code search"),
        Line::raw(""),
        Line::styled("Source reader", Style::new().add_modifier(Modifier::BOLD)),
        Line::raw("  Tab               Code → Blame → History"),
        Line::raw("  j/k               Move source cursor / history selection"),
        Line::raw("  v                 Start/end line-range selection"),
        Line::raw("  y                 Copy selected lines (or current line)"),
        Line::raw("  @                 Jump to function/type/symbol in current file"),
        Line::raw("  d                 Search definition/symbol across repository"),
        Line::raw("  Enter             Open commit from Blame/History"),
        Line::raw("  p                 Export print-ready HTML"),
        Line::raw(""),
        Line::styled("Authentication", Style::new().add_modifier(Modifier::BOLD)),
        Line::raw("  a / Ctrl+A        GitHub authentication menu"),
        Line::raw("  Ctrl+V            Paste from system clipboard in input dialogs"),
        Line::raw(""),
        Line::styled(
            format!(
                "Emoji mode resolved: {}",
                if app.icons.enabled { "on" } else { "off" }
            ),
            Style::new().fg(MUTED),
        ),
    ]
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

fn centered_window_start(index: usize, len: usize, visible: usize) -> usize {
    if len <= visible || visible == 0 {
        0
    } else {
        index.saturating_sub(visible / 2).min(len - visible)
    }
}

fn human_size(size: u64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MiB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.1} KiB", size as f64 / 1024.0)
    } else {
        format!("{size} B")
    }
}
