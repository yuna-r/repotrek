use chrono::Local;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};

use crate::{
    app::{App, CodeSearchMode, FileState, FileTab, HomeFocus, Modal, RepositoryTab, Screen},
    highlight::source_spans,
    model::{BlameRange, HistoryEntry, RepoCard},
    theme::Theme,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let theme = app.theme();
    frame.render_widget(
        Block::default().style(Style::new().bg(theme.background).fg(theme.text)),
        frame.area(),
    );

    let [header, content, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    draw_header(frame, header, app, theme);
    match app.screen {
        Screen::Home => draw_home(frame, content, app, theme),
        Screen::Repository => draw_repository(frame, content, app, theme),
        Screen::File => draw_file(frame, content, app, theme),
        Screen::Commit => draw_commit(frame, content, app, theme),
        Screen::Detail => draw_detail(frame, content, app, theme),
    }
    draw_footer(frame, footer, app, theme);

    if let Some(modal) = app.modal.as_ref() {
        draw_modal(frame, app, modal, theme);
    }
    if let Some(loading) = app.loading.as_ref() {
        draw_loading(frame, loading, theme);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let title = match app.screen {
        Screen::Home => "Home".to_owned(),
        Screen::Repository => app.repository.as_ref().map_or_else(
            || "Repository".to_owned(),
            |state| {
                format!(
                    "{}  {} {}",
                    state.repository.full_name, app.icons.branch, state.selected_ref
                )
            },
        ),
        Screen::File => app.repository.as_ref().map_or_else(
            || "File".to_owned(),
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
        Screen::Detail => app
            .detail
            .as_ref()
            .map_or_else(|| "Details".to_owned(), |detail| detail.document.title()),
    };

    let auth = if let Some(user) = app.auth_user.as_deref() {
        format!("GitHub @{user}")
    } else if app.authenticated {
        "GitHub authenticated".to_owned()
    } else {
        "GitHub anonymous · F2/a: authenticate".to_owned()
    };
    let rate = app.rate_limit.as_ref().and_then(|rate| {
        Some(format!(
            "{} {}/{}",
            rate.resource.as_deref().unwrap_or("api"),
            rate.remaining?,
            rate.limit?
        ))
    });
    let right = rate.map_or(auth.clone(), |rate| format!("{auth} · {rate}"));
    let right_width = right
        .chars()
        .count()
        .saturating_add(2)
        .min(area.width as usize) as u16;
    let [left, right_area] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(right_width)]).areas(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " RepoTrek ",
                Style::new()
                    .fg(theme.accent_text)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {title}"),
                Style::new()
                    .fg(theme.text)
                    .bg(theme.background)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Style::new().bg(theme.background)),
        left,
    );
    frame.render_widget(
        Paragraph::new(right)
            .alignment(Alignment::Right)
            .style(Style::new().fg(theme.muted).bg(theme.background)),
        right_area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let hint = match app.screen {
        Screen::Home => {
            "Enter open/search  ↑↓/Tab sections  F5 refresh  F2 auth  Esc clear/quit  Ctrl+Q quit"
        }
        Screen::Repository => {
            "←/→ tabs  ↑↓ select  Enter view  o GitHub  Esc/u parent  B branch  f files  s search"
        }
        Screen::File => {
            "Tab Code/Blame/History  ↑↓ move  Ctrl+↑↓ select  Ctrl+A all  Ctrl+C copy  w wrap  Esc parent"
        }
        Screen::Commit | Screen::Detail => {
            "↑↓ scroll  Ctrl+↑↓ select  Ctrl+A all  Ctrl+C copy  w wrap  o GitHub  Esc back"
        }
    };
    let text = app.status_text().unwrap_or(hint);
    let style = if app.status_text().is_some() {
        Style::new()
            .fg(theme.success)
            .bg(theme.background)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.muted).bg(theme.background)
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn draw_home(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let [search_area, note_area, lists_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .areas(area);

    let focused = app.home.focus == HomeFocus::Search;
    let title = app.icons.label(app.icons.search, "Repository or search");
    let block = themed_block(&title, focused, theme);
    let placeholder = if app.home.query.is_empty() {
        "owner/repo opens directly; every other value uses GitHub best-match search"
    } else {
        app.home.query.as_str()
    };
    frame.render_widget(
        Paragraph::new(format!("> {placeholder}"))
            .style(
                Style::new()
                    .fg(if app.home.query.is_empty() {
                        theme.muted
                    } else {
                        theme.text
                    })
                    .bg(theme.surface),
            )
            .block(block),
        search_area,
    );

    let auth_note = if app.authenticated {
        "GitHub authentication is active. Credentials are reused by GitHub CLI or the OS credential store."
    } else {
        "GitHub authentication is recommended: anonymous access is limited. Press F2 to sign in from anywhere."
    };
    frame.render_widget(
        Paragraph::new(auth_note).style(Style::new().fg(if app.authenticated {
            theme.muted
        } else {
            theme.warning
        })),
        note_area,
    );

    if lists_area.width >= 96 {
        let [history, featured, recommended] = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(lists_area);
        draw_history_list(frame, history, app, theme);
        draw_card_list(
            frame,
            featured,
            app,
            HomeFocus::Featured,
            "Featured",
            app.icons.featured,
            theme,
        );
        draw_card_list(
            frame,
            recommended,
            app,
            HomeFocus::Recommended,
            "Recommended",
            app.icons.recommended,
            theme,
        );
    } else {
        let [history, featured, recommended] = Layout::vertical([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(lists_area);
        draw_history_list(frame, history, app, theme);
        draw_card_list(
            frame,
            featured,
            app,
            HomeFocus::Featured,
            "Featured",
            app.icons.featured,
            theme,
        );
        draw_card_list(
            frame,
            recommended,
            app,
            HomeFocus::Recommended,
            "Recommended",
            app.icons.recommended,
            theme,
        );
    }
}

fn draw_history_list(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let focused = app.home.focus == HomeFocus::History;
    let title = if focused {
        app.icons
            .label(app.icons.history, "History · d delete · Ctrl+D clear")
    } else {
        app.icons.label(app.icons.history, "History")
    };
    let items = app
        .home
        .history
        .iter()
        .enumerate()
        .map(|(index, entry)| history_item(entry, index == app.home.history_index, focused, theme))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new(Line::styled(
                "No history yet",
                Style::new().fg(theme.muted),
            ))]
        } else {
            items
        })
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .block(themed_block(&title, focused, theme)),
        area,
    );
}

fn history_item(
    entry: &HistoryEntry,
    selected: bool,
    focused: bool,
    theme: Theme,
) -> ListItem<'static> {
    let location = entry.last_path.as_deref().unwrap_or("/");
    let time = entry.visited_at.with_timezone(&Local).format("%m-%d %H:%M");
    ListItem::new(vec![
        Line::styled(
            entry.repository.id.full_name(),
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            format!("  {location} · {time}"),
            Style::new().fg(theme.muted),
        ),
    ])
    .style(item_style(selected, focused, theme))
}

#[allow(clippy::too_many_arguments)]
fn draw_card_list(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    focus: HomeFocus,
    title: &str,
    icon: &str,
    theme: Theme,
) {
    let focused = app.home.focus == focus;
    let (cards, selected) = match focus {
        HomeFocus::Featured => (&app.home.featured, app.home.featured_index),
        HomeFocus::Recommended => (&app.home.recommended, app.home.recommended_index),
        HomeFocus::Search | HomeFocus::History => return,
    };
    let title = app.icons.label(icon, title);
    let items = cards
        .iter()
        .enumerate()
        .map(|(index, card)| card_item(card, index == selected, focused, app, theme))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new(Line::styled(
                "No results",
                Style::new().fg(theme.muted),
            ))]
        } else {
            items
        })
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .block(themed_block(&title, focused, theme)),
        area,
    );
}

fn card_item(
    card: &RepoCard,
    selected: bool,
    focused: bool,
    app: &App,
    theme: Theme,
) -> ListItem<'static> {
    let language = card.language.as_deref().unwrap_or("-");
    let stars = if card.stars == 0 {
        String::new()
    } else {
        format!(" {} {}", app.icons.star, card.stars)
    };
    ListItem::new(vec![
        Line::styled(
            card.id.full_name(),
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::styled(format!("  {language}{stars}"), Style::new().fg(theme.muted)),
    ])
    .style(item_style(selected, focused, theme))
}

fn themed_block<'a>(title: &'a str, focused: bool, theme: Theme) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .border_style(Style::new().fg(if focused { theme.accent } else { theme.border }))
        .title(title)
}

fn item_style(selected: bool, focused: bool, theme: Theme) -> Style {
    if selected && focused {
        Style::new().bg(theme.selection).fg(theme.text)
    } else if selected {
        Style::new().bg(theme.cursor).fg(theme.text)
    } else {
        Style::new().bg(theme.surface).fg(theme.text)
    }
}

fn draw_repository(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
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
                    Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "   {} {}   {} {}",
                        app.icons.star,
                        state.repository.stargazers_count,
                        app.icons.fork,
                        state.repository.forks_count
                    ),
                    Style::new().fg(theme.muted),
                ),
            ]),
            Line::styled(description.to_owned(), Style::new().fg(theme.muted)),
        ])
        .style(Style::new().bg(theme.background).fg(theme.text))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::new().fg(theme.border)),
        ),
        meta_area,
    );

    frame.render_widget(
        Tabs::new([
            "Code",
            "Commits",
            "Pull requests",
            "Issues",
            "Actions",
            "Releases",
        ])
        .select(state.tab.index())
        .divider("  ")
        .style(Style::new().fg(theme.muted).bg(theme.background))
        .highlight_style(
            Style::new()
                .fg(theme.accent)
                .bg(theme.background)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        tabs_area,
    );

    match state.tab {
        RepositoryTab::Code => draw_code_tree(frame, content_area, app, theme),
        RepositoryTab::Commits => draw_commits(frame, content_area, app, theme),
        RepositoryTab::PullRequests => draw_pull_requests(frame, content_area, app, theme),
        RepositoryTab::Issues => draw_issues(frame, content_area, app, theme),
        RepositoryTab::Actions => draw_actions(frame, content_area, app, theme),
        RepositoryTab::Releases => draw_releases(frame, content_area, app, theme),
    }
}

fn draw_code_tree(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
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
            let is_parent = entry.name == "..";
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
                    Style::new()
                        .fg(if entry.kind.is_directory() {
                            theme.accent
                        } else {
                            theme.text
                        })
                        .add_modifier(if is_parent {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(format!("  {size}"), Style::new().fg(theme.muted)),
            ]))
            .style(if start + offset == state.entry_index {
                Style::new().bg(theme.selection).fg(theme.text)
            } else {
                Style::new().bg(theme.surface).fg(theme.text)
            })
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new(Line::styled(
                "Empty directory",
                Style::new().fg(theme.muted),
            ))]
        } else {
            items
        })
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::new().bg(theme.surface).fg(theme.text))
                .border_style(Style::new().fg(theme.border))
                .title(format!("{} {path} · Esc/u/.. parent", app.icons.branch)),
        ),
        area,
    );
}

fn draw_commits(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let state = app.repository.as_ref().expect("repository state");
    if state.commits.is_empty() {
        frame.render_widget(
            Paragraph::new("Loading or no commits")
                .style(Style::new().fg(theme.muted).bg(theme.background)),
            area,
        );
        return;
    }
    if area.width >= 96 {
        let [list, preview] =
            Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)])
                .areas(area);
        draw_commit_list(frame, list, app, theme);
        draw_commit_preview(frame, preview, app, theme);
    } else {
        draw_commit_list(frame, area, app, theme);
    }
}

fn draw_commit_list(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
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
                    Span::styled(
                        format!("{} ", app.icons.commit),
                        Style::new().fg(theme.accent),
                    ),
                    Span::styled(
                        commit.title.clone(),
                        Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::styled(
                    format!(
                        "    {} · {date} · {}",
                        commit.author_name,
                        commit.short_sha()
                    ),
                    Style::new().fg(theme.muted),
                ),
            ])
            .style(if start + offset == state.commit_index {
                Style::new().bg(theme.selection).fg(theme.text)
            } else {
                Style::new().bg(theme.surface).fg(theme.text)
            })
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items)
            .style(Style::new().bg(theme.surface).fg(theme.text))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(theme.border))
                    .title(format!(
                        "Commits · page {} · Enter: details",
                        state.commit_page
                    )),
            ),
        area,
    );
}

fn draw_commit_preview(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
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
                Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            Line::styled(
                format!("{} · {date}", commit.author_name),
                Style::new().fg(theme.muted),
            ),
            Line::styled(
                format!("{} · {verified}", commit.sha),
                Style::new().fg(theme.muted),
            ),
            Line::raw(""),
            Line::raw(commit.body.clone()),
        ])
        .wrap(Wrap { trim: false })
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.border))
                .title("Preview"),
        ),
        area,
    );
}

fn draw_pull_requests(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let state = app.repository.as_ref().expect("repository state");

    if state.pull_requests.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    "No pull requests available",
                    Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::styled(
                    "GitHub returned no open pull requests for this repository. Pull requests may be disabled or restricted, or there may simply be none open.",
                    Style::new().fg(theme.muted),
                ),
                Line::raw(""),
                Line::styled(
                    "Press o to open the Pull requests page on GitHub.",
                    Style::new().fg(theme.accent),
                ),
            ])
            .wrap(Wrap { trim: false })
            .style(Style::new().bg(theme.surface).fg(theme.text))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(theme.border))
                    .title("Pull requests"),
            ),
            area,
        );
        return;
    }

    let items = state
        .pull_requests
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let draft = if item.draft { " · Draft" } else { "" };
            ListItem::new(vec![
                Line::styled(
                    format!("#{}  {}", item.number, item.title),
                    Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    format!(
                        "  {} · {} → {} · {} comments{draft}",
                        item.author, item.head, item.base, item.comments
                    ),
                    Style::new().fg(theme.muted),
                ),
            ])
            .style(if index == state.list_index {
                Style::new().bg(theme.selection).fg(theme.text)
            } else {
                Style::new().bg(theme.surface).fg(theme.text)
            })
        })
        .collect::<Vec<_>>();
    draw_generic_list(
        frame,
        area,
        "Open pull requests · Enter: view in RepoTrek · o: GitHub link",
        items,
        theme,
    );
}

fn draw_issues(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
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
                    Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    format!("  {} · {} comments{labels}", item.author, item.comments),
                    Style::new().fg(theme.muted),
                ),
            ])
            .style(if index == state.list_index {
                Style::new().bg(theme.selection).fg(theme.text)
            } else {
                Style::new().bg(theme.surface).fg(theme.text)
            })
        })
        .collect::<Vec<_>>();
    draw_generic_list(
        frame,
        area,
        "Open issues · Enter: view in RepoTrek · o: GitHub link",
        items,
        theme,
    );
}

fn draw_actions(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let state = app.repository.as_ref().expect("repository state");
    let items = state
        .workflow_runs
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let result = item.conclusion.as_deref().unwrap_or(&item.status);
            let (marker, color) = match result {
                "success" => ("✓", theme.success),
                "failure" | "cancelled" => ("✗", theme.danger),
                _ => ("●", theme.accent),
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("{marker} "), Style::new().fg(color)),
                    Span::styled(
                        item.name.clone(),
                        Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::styled(
                    format!(
                        "  {} · {} · {}",
                        item.event,
                        item.branch,
                        item.created_at.with_timezone(&Local).format("%m-%d %H:%M")
                    ),
                    Style::new().fg(theme.muted),
                ),
            ])
            .style(if index == state.list_index {
                Style::new().bg(theme.selection).fg(theme.text)
            } else {
                Style::new().bg(theme.surface).fg(theme.text)
            })
        })
        .collect::<Vec<_>>();
    draw_generic_list(
        frame,
        area,
        "Workflow runs · Enter: jobs/steps in RepoTrek · o: GitHub link",
        items,
        theme,
    );
}

fn draw_releases(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
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
                    format!("{}  {name}", item.tag_name),
                    Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Line::styled(format!("  {flags} · {date}"), Style::new().fg(theme.muted)),
            ])
            .style(if index == state.list_index {
                Style::new().bg(theme.selection).fg(theme.text)
            } else {
                Style::new().bg(theme.surface).fg(theme.text)
            })
        })
        .collect::<Vec<_>>();
    draw_generic_list(
        frame,
        area,
        "Releases · Enter: notes/assets in RepoTrek · o: GitHub link",
        items,
        theme,
    );
}

fn draw_generic_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: Vec<ListItem<'static>>,
    theme: Theme,
) {
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new(Line::styled(
                "No items",
                Style::new().fg(theme.muted),
            ))]
        } else {
            items
        })
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.border))
                .title(title),
        ),
        area,
    );
}

fn draw_file(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
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
            .style(Style::new().fg(theme.muted).bg(theme.background))
            .highlight_style(
                Style::new()
                    .fg(theme.accent)
                    .bg(theme.background)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        tabs_area,
    );
    match file.tab {
        FileTab::Code => draw_source_code(frame, content_area, app, false, theme),
        FileTab::Blame => draw_source_code(frame, content_area, app, true, theme),
        FileTab::History => draw_file_history(frame, content_area, app, theme),
    }
}

fn draw_source_code(frame: &mut Frame, area: Rect, app: &App, blame: bool, theme: Theme) {
    let file = app.file.as_ref().expect("file state");
    let total = file.line_count();
    let block_title = if file.selection_anchor.is_some() {
        let (start, end) = file.selection_range();
        format!(
            "{} · lines {}-{} selected · wrap {}",
            file.path,
            start + 1,
            end + 1,
            on_off(app.settings.wrap_code)
        )
    } else {
        format!(
            "{} · line {} / {} · wrap {}",
            file.path,
            file.cursor_line + 1,
            total,
            on_off(app.settings.wrap_code)
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .border_style(Style::new().fg(theme.border))
        .title(block_title);
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
    let (selection_start, selection_end) = file.selection_range();

    if app.settings.wrap_code {
        let lines = wrapped_source_lines(
            file,
            start,
            visible,
            blame,
            selection_start,
            selection_end,
            theme,
        );
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false })
                .style(Style::new().bg(theme.surface).fg(theme.text)),
            inner,
        );
        return;
    }

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
                let bg = line_background(file, line, selection_start, selection_end, theme);
                Line::styled(
                    format!("{sha:<8} {author:<18.18} {:>5} │", line + 1),
                    Style::new().fg(theme.muted).bg(bg),
                )
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(blame_lines).style(Style::new().bg(theme.surface)),
            blame_area,
        );
        let source = source_lines(file, start, visible, selection_start, selection_end, theme);
        frame.render_widget(
            Paragraph::new(source)
                .scroll((0, file.horizontal_scroll.min(u16::MAX as usize) as u16))
                .style(Style::new().bg(theme.surface)),
            source_area,
        );
    } else {
        let number_width = total.to_string().len().clamp(3, 8) as u16 + 3;
        let [numbers_area, source_area] =
            Layout::horizontal([Constraint::Length(number_width), Constraint::Min(1)]).areas(inner);
        let numbers = (start..start + visible)
            .take_while(|line| *line < total)
            .map(|line| {
                let bg = line_background(file, line, selection_start, selection_end, theme);
                Line::styled(
                    format!(
                        "{:>width$} │",
                        line + 1,
                        width = number_width.saturating_sub(2) as usize
                    ),
                    Style::new().fg(theme.muted).bg(bg),
                )
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(numbers).style(Style::new().bg(theme.surface)),
            numbers_area,
        );
        let source = source_lines(file, start, visible, selection_start, selection_end, theme);
        frame.render_widget(
            Paragraph::new(source)
                .scroll((0, file.horizontal_scroll.min(u16::MAX as usize) as u16))
                .style(Style::new().bg(theme.surface)),
            source_area,
        );
    }
}

fn wrapped_source_lines(
    file: &FileState,
    start: usize,
    visible: usize,
    blame: bool,
    selection_start: usize,
    selection_end: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let number_width = file.line_count().to_string().len().clamp(3, 8);
    file.content
        .lines()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, line)| {
            let line_index = start + offset;
            let bg = line_background(file, line_index, selection_start, selection_end, theme);
            let mut spans = Vec::new();
            if blame {
                let range = blame_for_line(&file.blame, line_index + 1);
                let (sha, author) = range.map_or(("-------", "unknown"), |range| {
                    (range.commit_short_sha.as_str(), range.author.as_str())
                });
                spans.push(Span::styled(
                    format!("{sha:<8} {author:<14.14} "),
                    Style::new().fg(theme.muted).bg(bg),
                ));
            }
            spans.push(Span::styled(
                format!("{:>number_width$} │ ", line_index + 1),
                Style::new().fg(theme.muted).bg(bg),
            ));
            spans.extend(
                source_spans(line, &file.path, theme)
                    .into_iter()
                    .map(|mut span| {
                        span.style = span.style.patch(Style::new().bg(bg));
                        span
                    }),
            );
            Line::from(spans)
        })
        .collect()
}

fn source_lines(
    file: &FileState,
    start: usize,
    visible: usize,
    selection_start: usize,
    selection_end: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    file.content
        .lines()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, line)| {
            let line_index = start + offset;
            let bg = line_background(file, line_index, selection_start, selection_end, theme);
            let spans = source_spans(line, &file.path, theme)
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
    file: &FileState,
    line: usize,
    selection_start: usize,
    selection_end: usize,
    theme: Theme,
) -> ratatui::style::Color {
    if file.selection_anchor.is_some() && line >= selection_start && line <= selection_end {
        theme.selection
    } else if line == file.cursor_line {
        theme.cursor
    } else {
        theme.surface
    }
}

fn blame_for_line(ranges: &[BlameRange], line: usize) -> Option<&BlameRange> {
    ranges
        .iter()
        .find(|range| line >= range.starting_line && line <= range.ending_line)
}

fn draw_file_history(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
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
                    Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    format!("  {} · {date}", commit.author_name),
                    Style::new().fg(theme.muted),
                ),
            ])
            .style(if index == file.history_index {
                Style::new().bg(theme.selection).fg(theme.text)
            } else {
                Style::new().bg(theme.surface).fg(theme.text)
            })
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(if items.is_empty() {
            vec![ListItem::new(Line::styled(
                "No history loaded",
                Style::new().fg(theme.muted),
            ))]
        } else {
            items
        })
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(theme.border))
                .title(format!("History · {} · Enter: commit", file.path)),
        ),
        area,
    );
}

fn draw_commit(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let Some(commit) = app.commit.as_ref() else {
        frame.render_widget(Paragraph::new("Commit state is unavailable"), area);
        return;
    };
    draw_reader(
        frame,
        area,
        &commit.text(),
        &commit.detail.summary.title,
        commit.cursor_line,
        commit.viewport_top,
        commit.horizontal_scroll,
        commit.selection_anchor,
        app.settings.wrap_diff,
        theme,
    );
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let Some(detail) = app.detail.as_ref() else {
        frame.render_widget(Paragraph::new("Detail state is unavailable"), area);
        return;
    };
    draw_reader(
        frame,
        area,
        &detail.document.text(),
        &format!("{} · o: GitHub link", detail.document.title()),
        detail.cursor_line,
        detail.viewport_top,
        detail.horizontal_scroll,
        detail.selection_anchor,
        app.settings.wrap_diff,
        theme,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_reader(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    title: &str,
    cursor_line: usize,
    viewport_top: usize,
    horizontal_scroll: usize,
    selection_anchor: Option<usize>,
    wrap: bool,
    theme: Theme,
) {
    let total = text.lines().count().max(1);
    let selection =
        selection_anchor.map(|anchor| (anchor.min(cursor_line), anchor.max(cursor_line)));
    let block_title = selection.map_or_else(
        || {
            format!(
                "{title} · line {} / {total} · wrap {}",
                cursor_line + 1,
                on_off(wrap)
            )
        },
        |(start, end)| {
            format!(
                "{title} · lines {}-{} selected · wrap {}",
                start + 1,
                end + 1,
                on_off(wrap)
            )
        },
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::new().bg(theme.surface).fg(theme.text))
        .border_style(Style::new().fg(theme.border))
        .title(block_title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let visible = inner.height as usize;
    let mut start = viewport_top.min(total.saturating_sub(1));
    if cursor_line < start {
        start = cursor_line;
    }
    if cursor_line >= start + visible {
        start = cursor_line.saturating_sub(visible.saturating_sub(1));
    }
    let lines = reader_lines(text, start, visible, cursor_line, selection, theme);
    let mut paragraph =
        Paragraph::new(Text::from(lines)).style(Style::new().bg(theme.surface).fg(theme.text));
    if wrap {
        paragraph = paragraph.wrap(Wrap { trim: false });
    } else {
        paragraph = paragraph.scroll((0, horizontal_scroll.min(u16::MAX as usize) as u16));
    }
    frame.render_widget(paragraph, inner);
}

fn reader_lines(
    text: &str,
    start: usize,
    visible: usize,
    cursor_line: usize,
    selection: Option<(usize, usize)>,
    theme: Theme,
) -> Vec<Line<'static>> {
    let all = text.lines().collect::<Vec<_>>();
    let mut current_path = String::new();
    for line in all.iter().take(start) {
        if let Some(path) = parse_file_header(line) {
            current_path = path;
        }
    }

    all.into_iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, line)| {
            let line_index = start + offset;
            if let Some(path) = parse_file_header(line) {
                current_path = path;
            }
            reader_line(
                line,
                &current_path,
                line_index,
                cursor_line,
                selection,
                theme,
            )
        })
        .collect()
}

fn reader_line(
    line: &str,
    path: &str,
    line_index: usize,
    cursor_line: usize,
    selection: Option<(usize, usize)>,
    theme: Theme,
) -> Line<'static> {
    let selected = selection.is_some_and(|(start, end)| line_index >= start && line_index <= end);
    let base_bg = if selected {
        theme.selection
    } else if line_index == cursor_line {
        theme.cursor
    } else {
        theme.surface
    };

    if line.starts_with("@@") {
        return Line::styled(
            line.to_owned(),
            Style::new().fg(theme.accent).bg(if selected {
                theme.selection
            } else {
                theme.diff_hunk_bg
            }),
        );
    }
    if line.starts_with("--- ") {
        return Line::styled(
            line.to_owned(),
            Style::new()
                .fg(theme.accent)
                .bg(base_bg)
                .add_modifier(Modifier::BOLD),
        );
    }
    if let Some((prefix, sign, source)) = split_numbered_diff(line) {
        let diff_bg = if selected {
            theme.selection
        } else {
            match sign {
                '+' => theme.diff_add_bg,
                '-' => theme.diff_delete_bg,
                _ => base_bg,
            }
        };
        let sign_color = match sign {
            '+' => theme.success,
            '-' => theme.danger,
            _ => theme.muted,
        };
        let mut spans = vec![Span::styled(
            format!("{prefix}{sign} "),
            Style::new().fg(sign_color).bg(diff_bg),
        )];
        spans.extend(
            source_spans(source, path, theme)
                .into_iter()
                .map(|mut span| {
                    span.style = span.style.patch(Style::new().bg(diff_bg));
                    span
                }),
        );
        return Line::from(spans);
    }
    if line.starts_with("Comments")
        || line.starts_with("Assets")
        || line.starts_with("Job:")
        || line.starts_with("State:")
    {
        return Line::styled(
            line.to_owned(),
            Style::new()
                .fg(theme.text)
                .bg(base_bg)
                .add_modifier(Modifier::BOLD),
        );
    }
    Line::styled(line.to_owned(), Style::new().fg(theme.text).bg(base_bg))
}

fn parse_file_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("--- ")?;
    Some(rest.split(" · ").next().unwrap_or(rest).trim().to_owned())
}

fn split_numbered_diff(line: &str) -> Option<(&str, char, &str)> {
    if line.len() < 14 {
        return None;
    }
    let bytes = line.as_bytes();
    let sign_index = 12;
    let sign = *bytes.get(sign_index)? as char;
    if !matches!(sign, '+' | '-' | ' ') || bytes.get(sign_index + 1) != Some(&b' ') {
        return None;
    }
    Some((&line[..sign_index], sign, &line[sign_index + 2..]))
}

fn draw_modal(frame: &mut Frame, app: &App, modal: &Modal, theme: Theme) {
    match modal {
        Modal::Help => draw_text_modal(frame, "Help", help_lines(app, theme), 88, 32, theme),
        Modal::ConfirmClearHistory => draw_text_modal(
            frame,
            "Clear browsing history?",
            vec![
                Line::raw("Delete all repository browsing history from this device?"),
                Line::raw(""),
                Line::styled(
                    "Enter / y  Clear all history",
                    Style::new().fg(theme.danger),
                ),
                Line::styled("Esc / n    Cancel", Style::new().fg(theme.muted)),
            ],
            62,
            9,
            theme,
        ),
        Modal::Settings { index } => {
            let options = [
                format!("Theme               {}", app.settings.theme.label()),
                format!("Source wrapping      {}", on_off(app.settings.wrap_code)),
                format!("Diff/detail wrapping {}", on_off(app.settings.wrap_diff)),
            ];
            let mut lines = options
                .into_iter()
                .enumerate()
                .map(|(position, option)| {
                    Line::styled(
                        option,
                        if position == *index {
                            Style::new().bg(theme.selection).fg(theme.text)
                        } else {
                            Style::new().fg(theme.text)
                        },
                    )
                })
                .collect::<Vec<_>>();
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "↑↓/Tab select · Enter/←/→ toggle · T toggles theme anywhere",
                Style::new().fg(theme.muted),
            ));
            draw_text_modal(frame, "Settings", lines, 68, 11, theme);
        }
        Modal::Error { title, message } => {
            let mut lines = message
                .lines()
                .map(|line| Line::raw(line.to_owned()))
                .collect::<Vec<_>>();
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Enter / Esc close",
                Style::new().fg(theme.accent),
            ));
            draw_text_modal(frame, title, lines, 76, 12, theme);
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
                    Line::styled(
                        "Enter / a  Authentication options",
                        Style::new().fg(theme.accent),
                    ),
                    Line::styled(
                        "Esc        Continue with loaded data",
                        Style::new().fg(theme.muted),
                    ),
                ],
                68,
                10,
                theme,
            );
        }
        Modal::AuthMenu { index } => {
            let options = [
                "1  Sign in with GitHub CLI / browser (persistent)",
                "2  Paste Personal Access Token for this session",
                "3  Paste token and store with GitHub CLI / OS credential store",
            ];
            let mut lines = options
                .iter()
                .enumerate()
                .map(|(position, option)| {
                    Line::styled(
                        (*option).to_owned(),
                        if position == *index {
                            Style::new().bg(theme.selection).fg(theme.text)
                        } else {
                            Style::new().fg(theme.text)
                        },
                    )
                })
                .collect::<Vec<_>>();
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "RepoTrek never writes a token to history.json or settings.json.",
                Style::new().fg(theme.muted),
            ));
            draw_text_modal(frame, "GitHub authentication", lines, 86, 12, theme);
        }
        Modal::TokenInput { input, persist } => {
            let masked = "•".repeat(input.chars().count().min(72));
            let mode = if *persist {
                "persistent GitHub CLI / OS credential store"
            } else {
                "session only"
            };
            draw_text_modal(
                frame,
                "GitHub token",
                vec![
                    Line::styled(format!("Store: {mode}"), Style::new().fg(theme.muted)),
                    Line::raw(""),
                    Line::styled(format!("> {masked}"), Style::new().fg(theme.text)),
                    Line::raw(""),
                    Line::styled(
                        "Type or Ctrl+V paste · Enter connect · Esc cancel",
                        Style::new().fg(theme.accent),
                    ),
                ],
                86,
                10,
                theme,
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
            let visible = 20;
            let start = centered_window_start(*index, filtered.len(), visible);
            let lines = filtered
                .iter()
                .skip(start)
                .take(visible)
                .enumerate()
                .map(|(offset, branch)| {
                    let protected = if branch.protected { " [protected]" } else { "" };
                    Line::styled(
                        format!("{}{protected}", branch.name),
                        palette_item_style(start + offset == *index, theme),
                    )
                })
                .collect::<Vec<_>>();
            draw_palette(frame, "Switch branch", query, lines, 82, 26, theme);
        }
        Modal::RepositorySearch {
            query,
            results,
            index,
        } => {
            let visible = 20;
            let start = centered_window_start(*index, results.len(), visible);
            let lines = results
                .iter()
                .skip(start)
                .take(visible)
                .enumerate()
                .map(|(offset, repository)| {
                    Line::styled(
                        format!(
                            "{:<40}  ★ {:>7}  {}",
                            repository.id.full_name(),
                            repository.stars,
                            repository.language.as_deref().unwrap_or("-")
                        ),
                        palette_item_style(start + offset == *index, theme),
                    )
                })
                .collect();
            draw_palette(
                frame,
                "Search repositories · GitHub best match + local relevance reranking",
                query,
                lines,
                92,
                27,
                theme,
            );
        }
        Modal::FileSearch {
            query,
            results,
            index,
            ..
        } => {
            let visible = 22;
            let start = centered_window_start(*index, results.len(), visible);
            let lines = results
                .iter()
                .skip(start)
                .take(visible)
                .enumerate()
                .map(|(offset, path)| {
                    Line::styled(
                        path.clone(),
                        palette_item_style(start + offset == *index, theme),
                    )
                })
                .collect();
            draw_palette(frame, "Go to file", query, lines, 92, 28, theme);
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
            let visible = 22;
            let start = centered_window_start(*index, results.len(), visible);
            let lines = results
                .iter()
                .skip(start)
                .take(visible)
                .enumerate()
                .map(|(offset, result)| {
                    Line::styled(
                        result.path.clone(),
                        palette_item_style(start + offset == *index, theme),
                    )
                })
                .collect();
            draw_palette(frame, title, query, lines, 92, 28, theme);
        }
        Modal::SymbolPicker {
            query,
            results,
            index,
            ..
        } => {
            let visible = 22;
            let start = centered_window_start(*index, results.len(), visible);
            let lines = results
                .iter()
                .skip(start)
                .take(visible)
                .enumerate()
                .map(|(offset, symbol)| {
                    Line::styled(
                        format!("{:>5}  {:<10} {}", symbol.line, symbol.kind, symbol.name),
                        palette_item_style(start + offset == *index, theme),
                    )
                })
                .collect();
            draw_palette(frame, "Symbols in file", query, lines, 78, 28, theme);
        }
    }
}

fn palette_item_style(selected: bool, theme: Theme) -> Style {
    if selected {
        Style::new().bg(theme.selection).fg(theme.text)
    } else {
        Style::new().bg(theme.surface).fg(theme.text)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_palette(
    frame: &mut Frame,
    title: &str,
    query: &str,
    mut lines: Vec<Line<'static>>,
    width_percent: u16,
    height: u16,
    theme: Theme,
) {
    let mut content = vec![
        Line::styled(
            format!("> {query}"),
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            "Enter search/open · ↑↓/Tab select · type to refine · Ctrl+V paste · Esc close",
            Style::new().fg(theme.muted),
        ),
        Line::raw(""),
    ];
    if lines.is_empty() {
        lines.push(Line::styled("No results yet", Style::new().fg(theme.muted)));
    }
    content.extend(lines);
    draw_text_modal(frame, title, content, width_percent, height, theme);
}

fn draw_text_modal(
    frame: &mut Frame,
    title: &str,
    lines: Vec<Line<'static>>,
    width_percent: u16,
    height: u16,
    theme: Theme,
) {
    let area = centered_rect(width_percent, height, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::new().bg(theme.surface).fg(theme.text))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(Style::new().bg(theme.surface).fg(theme.text))
                    .border_style(Style::new().fg(theme.accent))
                    .title(title),
            ),
        area,
    );
}

fn draw_loading(frame: &mut Frame, message: &str, theme: Theme) {
    let area = centered_rect(56, 5, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!("●  {message}"))
            .alignment(Alignment::Center)
            .style(
                Style::new()
                    .fg(theme.accent)
                    .bg(theme.surface)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(Style::new().bg(theme.surface))
                    .border_style(Style::new().fg(theme.accent)),
            ),
        area,
    );
}

fn help_lines(app: &App, theme: Theme) -> Vec<Line<'static>> {
    vec![
        Line::styled(
            "Home",
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::raw("  owner/repo        Directly open exactly one repository"),
        Line::raw("  other text        GitHub best-match repository search"),
        Line::raw("  ↑↓ / Tab          Move consistently between sections and rows"),
        Line::raw("  d                 Delete selected History entry"),
        Line::raw("  Ctrl+D            Clear all History (confirmation required)"),
        Line::raw("  F5 / Ctrl+R       Refresh featured/recommended"),
        Line::raw("  Esc               Clear query; on empty query quit"),
        Line::raw(""),
        Line::styled(
            "Repository",
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::raw(
            "  ←/→ or h/l        Move Code, Commits, Pull requests, Issues, Actions, Releases",
        ),
        Line::raw("  1..6              Open a repository tab"),
        Line::raw("  Enter             Open selected item inside RepoTrek"),
        Line::raw("  o                 Open selected item on GitHub (explicit external link)"),
        Line::raw("  Esc / u / ..      Go to parent directory"),
        Line::raw("  B                 Switch branch"),
        Line::raw("  f                 Recursive file finder"),
        Line::raw("  s or /            Repository full-text code search"),
        Line::raw(""),
        Line::styled(
            "Readers",
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::raw("  ↑↓                Move source/detail cursor"),
        Line::raw("  Ctrl+↑ / Ctrl+↓   Extend line selection"),
        Line::raw("  Ctrl+A            Select all lines"),
        Line::raw("  Ctrl+C            Copy selection to system clipboard"),
        Line::raw("  v / y             Vim-style selection / copy remains available"),
        Line::raw("  w                 Toggle right wrapping for source or diff/detail"),
        Line::raw("  @                 Jump to function/type/symbol in current file"),
        Line::raw("  d                 Search definition/symbol across repository"),
        Line::raw("  p                 Export print-ready HTML"),
        Line::raw(""),
        Line::styled(
            "Authentication and appearance",
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::raw("  F2 / a            GitHub authentication menu"),
        Line::raw("  ,                 Settings"),
        Line::raw("  T                 Toggle Dark / Light theme"),
        Line::raw("  Ctrl+Q            Quit from anywhere"),
        Line::raw(""),
        Line::styled(
            format!(
                "Theme: {} · Source wrap: {} · Diff wrap: {} · Emoji: {}",
                app.settings.theme.label(),
                on_off(app.settings.wrap_code),
                on_off(app.settings.wrap_diff),
                if app.icons.enabled { "on" } else { "off" }
            ),
            Style::new().fg(theme.muted),
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

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}
