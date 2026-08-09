use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{
        App, AppCommand, CodeSearchMode, FileState, FileTab, HomeFocus, Modal, RepositoryTab,
        Screen,
    },
    highlight::{source_spans, source_spans_with_language},
    language::detect_language,
    model::{BlameRange, HistoryEntry, RepoCard},
    settings::FooterMode,
    theme::Theme,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let theme = app.theme();
    frame.render_widget(
        Block::default().style(Style::new().bg(theme.background).fg(theme.text)),
        frame.area(),
    );

    let header_height = header_height(app, frame.area().width);
    let footer_height = footer_height(app, frame.area().width);
    let [header, content, footer] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(5),
        Constraint::Length(footer_height),
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

pub fn handle_mouse(app: &mut App, mouse: MouseEvent, width: u16, height: u16) -> AppCommand {
    if app.loading.is_some() {
        return AppCommand::None;
    }

    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    match mouse.kind {
        MouseEventKind::ScrollUp => app.handle_key(key(KeyCode::Up)),
        MouseEventKind::ScrollDown => app.handle_key(key(KeyCode::Down)),
        MouseEventKind::ScrollLeft => app.handle_key(key(KeyCode::Left)),
        MouseEventKind::ScrollRight => app.handle_key(key(KeyCode::Right)),
        MouseEventKind::Down(MouseButton::Right) => app.handle_key(key(KeyCode::Esc)),
        MouseEventKind::Down(MouseButton::Left) => {
            let outer = Rect::new(0, 0, width, height);
            handle_left_click(app, mouse.column, mouse.row, mouse.modifiers, outer)
        }
        _ => AppCommand::None,
    }
}

fn handle_left_click(
    app: &mut App,
    column: u16,
    row: u16,
    modifiers: KeyModifiers,
    outer: Rect,
) -> AppCommand {
    if app.modal.is_some() {
        return handle_modal_click(app, column, row, outer);
    }

    let header_size = header_height(app, outer.width);
    let footer_size = footer_height(app, outer.width);
    let [_header, content, footer] = Layout::vertical([
        Constraint::Length(header_size),
        Constraint::Min(5),
        Constraint::Length(footer_size),
    ])
    .areas(outer);

    if point_in(footer, column, row) {
        let layout = footer_action_layout(app, footer.width);
        let relative_row = row.saturating_sub(footer.y);
        let relative_column = column.saturating_sub(footer.x);
        if let Some(hit) = layout.hits.into_iter().find(|hit| {
            hit.row == relative_row && relative_column >= hit.start && relative_column < hit.end
        }) {
            return app.handle_key(hit.event);
        }
        return AppCommand::None;
    }

    if !point_in(content, column, row) {
        return AppCommand::None;
    }

    match app.screen {
        Screen::Home => handle_home_click(app, content, column, row),
        Screen::Repository => handle_repository_click(app, content, column, row),
        Screen::File => handle_file_click(app, content, column, row, modifiers),
        Screen::Commit | Screen::Detail => {
            handle_reader_click(app, content, row, modifiers);
            AppCommand::None
        }
    }
}

fn handle_home_click(app: &mut App, area: Rect, column: u16, row: u16) -> AppCommand {
    let [search_area, _note_area, lists_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .areas(area);

    if point_in(search_area, column, row) {
        app.home.focus = HomeFocus::Search;
        return AppCommand::None;
    }

    let list_areas = if lists_area.width >= 96 {
        let [history, featured, recommended] = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(lists_area);
        [history, featured, recommended]
    } else {
        let [history, featured, recommended] = Layout::vertical([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(lists_area);
        [history, featured, recommended]
    };

    for (focus, list_area) in [
        (HomeFocus::History, list_areas[0]),
        (HomeFocus::Featured, list_areas[1]),
        (HomeFocus::Recommended, list_areas[2]),
    ] {
        if !point_in(list_area, column, row) {
            continue;
        }
        let (current, len) = match focus {
            HomeFocus::History => (app.home.history_index, app.home.history.len()),
            HomeFocus::Featured => (app.home.featured_index, app.home.featured.len()),
            HomeFocus::Recommended => (app.home.recommended_index, app.home.recommended.len()),
            HomeFocus::Search => (0, 0),
        };
        let visible = visible_two_line_items(list_area);
        let start = centered_window_start(current, len, visible);
        let Some(index) = list_index_at(list_area, row, start, 2, len) else {
            app.home.focus = focus;
            return AppCommand::None;
        };
        let activate = app.home.focus == focus && current == index;
        app.home.focus = focus;
        match focus {
            HomeFocus::History => app.home.history_index = index,
            HomeFocus::Featured => app.home.featured_index = index,
            HomeFocus::Recommended => app.home.recommended_index = index,
            HomeFocus::Search => {}
        }
        return if activate {
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        } else {
            AppCommand::None
        };
    }

    AppCommand::None
}

fn handle_repository_click(app: &mut App, area: Rect, column: u16, row: u16) -> AppCommand {
    let [_meta_area, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .areas(area);

    if point_in(tabs_area, column, row) {
        if let Some(index) = tab_index_at(
            tabs_area,
            column,
            &[
                "Code",
                "Commits",
                "Pull requests",
                "Issues",
                "Actions",
                "Releases",
            ],
        ) {
            let code = char::from(b'1' + index as u8);
            return app.handle_key(KeyEvent::new(KeyCode::Char(code), KeyModifiers::NONE));
        }
        return AppCommand::None;
    }

    if !point_in(content_area, column, row) {
        return AppCommand::None;
    }

    let Some(state) = app.repository.as_ref() else {
        return AppCommand::None;
    };
    let tab = state.tab;
    let (current, len, list_area, item_height) = match tab {
        RepositoryTab::Code => (state.entry_index, state.entries.len(), content_area, 1_u16),
        RepositoryTab::Commits => {
            let list_area = if content_area.width >= 96 {
                let [list_area, _preview_area] =
                    Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)])
                        .areas(content_area);
                list_area
            } else {
                content_area
            };
            (state.commit_index, state.commits.len(), list_area, 2)
        }
        RepositoryTab::PullRequests => {
            (state.list_index, state.pull_requests.len(), content_area, 2)
        }
        RepositoryTab::Issues => (state.list_index, state.issues.len(), content_area, 2),
        RepositoryTab::Actions => (state.list_index, state.workflow_runs.len(), content_area, 2),
        RepositoryTab::Releases => (state.list_index, state.releases.len(), content_area, 2),
    };
    if !point_in(list_area, column, row) {
        return AppCommand::None;
    }
    let visible = if item_height == 1 {
        usize::from(list_area.height.saturating_sub(2)).max(1)
    } else {
        visible_two_line_items(list_area)
    };
    let start = centered_window_start(current, len, visible);
    let Some(index) = list_index_at(list_area, row, start, item_height, len) else {
        return AppCommand::None;
    };
    let activate = current == index;
    if let Some(state) = app.repository.as_mut() {
        match tab {
            RepositoryTab::Code => state.entry_index = index,
            RepositoryTab::Commits => state.commit_index = index,
            RepositoryTab::PullRequests
            | RepositoryTab::Issues
            | RepositoryTab::Actions
            | RepositoryTab::Releases => state.list_index = index,
        }
    }
    if activate {
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
    } else {
        AppCommand::None
    }
}

fn handle_file_click(
    app: &mut App,
    area: Rect,
    column: u16,
    row: u16,
    modifiers: KeyModifiers,
) -> AppCommand {
    let [tabs_area, content_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
    if point_in(tabs_area, column, row) {
        if let Some(index) = tab_index_at(tabs_area, column, &["Code", "Blame", "History"]) {
            let code = char::from(b'1' + index as u8);
            return app.handle_key(KeyEvent::new(KeyCode::Char(code), KeyModifiers::NONE));
        }
        return AppCommand::None;
    }

    if !point_in(content_area, column, row) {
        return AppCommand::None;
    }
    let Some(file) = app.file.as_ref() else {
        return AppCommand::None;
    };

    if file.tab == FileTab::History {
        let visible = visible_two_line_items(content_area);
        let start = centered_window_start(file.history_index, file.history.len(), visible);
        let Some(index) = list_index_at(content_area, row, start, 2, file.history.len()) else {
            return AppCommand::None;
        };
        let activate = file.history_index == index;
        if let Some(file) = app.file.as_mut() {
            file.history_index = index;
        }
        return if activate {
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        } else {
            AppCommand::None
        };
    }

    let inner = bordered_inner(content_area);
    if !point_in(inner, column, row) {
        return AppCommand::None;
    }
    let total = file.line_count();
    let visible = usize::from(inner.height).max(1);
    let mut start = file.viewport_top.min(total.saturating_sub(1));
    if file.cursor_line < start {
        start = file.cursor_line;
    }
    if file.cursor_line >= start + visible {
        start = file.cursor_line.saturating_sub(visible.saturating_sub(1));
    }
    let clicked_row = usize::from(row.saturating_sub(inner.y));
    let line = if app.settings.wrap_code {
        let number_width = total.to_string().len().clamp(3, 8);
        let prefix_width = number_width + 3 + if file.tab == FileTab::Blame { 24 } else { 0 };
        wrapped_logical_line_at_row(
            &file.content,
            start,
            clicked_row,
            usize::from(inner.width.max(1)),
            prefix_width,
        )
    } else {
        start.saturating_add(clicked_row)
    }
    .min(total.saturating_sub(1));
    if let Some(file) = app.file.as_mut() {
        if modifiers.contains(KeyModifiers::SHIFT) {
            file.selection_anchor.get_or_insert(file.cursor_line);
        } else {
            file.selection_anchor = None;
        }
        file.cursor_line = line;
        file.viewport_top = start;
    }
    AppCommand::None
}

fn handle_reader_click(app: &mut App, area: Rect, row: u16, modifiers: KeyModifiers) {
    let inner = bordered_inner(area);
    if row < inner.y || row >= inner.y.saturating_add(inner.height) {
        return;
    }
    let clicked_row = usize::from(row.saturating_sub(inner.y));
    let wrap = app.settings.wrap_diff;
    match app.screen {
        Screen::Commit => {
            let Some(commit) = app.commit.as_mut() else {
                return;
            };
            let text = commit.text();
            let total = text.lines().count().max(1);
            let visible = usize::from(inner.height).max(1);
            let mut start = commit.viewport_top.min(total.saturating_sub(1));
            if commit.cursor_line < start {
                start = commit.cursor_line;
            }
            if commit.cursor_line >= start + visible {
                start = commit.cursor_line.saturating_sub(visible.saturating_sub(1));
            }
            if modifiers.contains(KeyModifiers::SHIFT) {
                commit.selection_anchor.get_or_insert(commit.cursor_line);
            } else {
                commit.selection_anchor = None;
            }
            commit.cursor_line = if wrap {
                wrapped_logical_line_at_row(
                    &text,
                    start,
                    clicked_row,
                    usize::from(inner.width.max(1)),
                    0,
                )
            } else {
                start.saturating_add(clicked_row)
            }
            .min(total.saturating_sub(1));
            commit.viewport_top = start;
        }
        Screen::Detail => {
            let Some(detail) = app.detail.as_mut() else {
                return;
            };
            let text = detail.document.text();
            let total = text.lines().count().max(1);
            let visible = usize::from(inner.height).max(1);
            let mut start = detail.viewport_top.min(total.saturating_sub(1));
            if detail.cursor_line < start {
                start = detail.cursor_line;
            }
            if detail.cursor_line >= start + visible {
                start = detail.cursor_line.saturating_sub(visible.saturating_sub(1));
            }
            if modifiers.contains(KeyModifiers::SHIFT) {
                detail.selection_anchor.get_or_insert(detail.cursor_line);
            } else {
                detail.selection_anchor = None;
            }
            detail.cursor_line = if wrap {
                wrapped_logical_line_at_row(
                    &text,
                    start,
                    clicked_row,
                    usize::from(inner.width.max(1)),
                    0,
                )
            } else {
                start.saturating_add(clicked_row)
            }
            .min(total.saturating_sub(1));
            detail.viewport_top = start;
        }
        Screen::Home | Screen::Repository | Screen::File => {}
    }
}

fn handle_modal_click(app: &mut App, column: u16, row: u16, outer: Rect) -> AppCommand {
    let Some(modal) = app.modal.clone() else {
        return AppCommand::None;
    };

    let palette = match &modal {
        Modal::BranchPicker {
            query,
            branches,
            index,
        } => {
            let normalized = query.to_ascii_lowercase();
            let len = branches
                .iter()
                .filter(|branch| {
                    normalized.is_empty() || branch.name.to_ascii_lowercase().contains(&normalized)
                })
                .count();
            Some((82, 26, query.clone(), len, *index, 20))
        }
        Modal::RepositorySearch {
            query,
            results,
            index,
        } => Some((92, 27, query.clone(), results.len(), *index, 20)),
        Modal::FileSearch {
            query,
            results,
            index,
            ..
        } => Some((92, 28, query.clone(), results.len(), *index, 22)),
        Modal::CodeSearch {
            query,
            results,
            index,
            ..
        } => Some((96, 28, query.clone(), results.len(), *index, 22)),
        Modal::SymbolPicker {
            query,
            results,
            index,
            ..
        } => Some((88, 28, query.clone(), results.len(), *index, 22)),
        Modal::FindInFile {
            query,
            matches,
            index,
        } => Some((92, 28, query.clone(), matches.len(), *index, 22)),
        _ => None,
    };

    if let Some((width_percent, height, query, len, current, visible)) = palette {
        let modal_area = centered_rect(width_percent, height, outer);
        if !point_in(modal_area, column, row) {
            return app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        }
        let inner_width = modal_area.width.saturating_sub(2).max(1);
        let query_height = wrapped_line_count(&format!("> {query}"), inner_width);
        let instruction_height = wrapped_line_count(PALETTE_INSTRUCTION, inner_width);
        let first_result_row = modal_area
            .y
            .saturating_add(1)
            .saturating_add(query_height)
            .saturating_add(instruction_height)
            .saturating_add(1);
        if row < first_result_row || len == 0 {
            return AppCommand::None;
        }
        let start = centered_window_start(current, len, visible);
        let clicked = start.saturating_add(usize::from(row.saturating_sub(first_result_row)));
        if clicked >= len || clicked >= start.saturating_add(visible) {
            return AppCommand::None;
        }
        match app.modal.as_mut() {
            Some(Modal::BranchPicker { index, .. })
            | Some(Modal::RepositorySearch { index, .. })
            | Some(Modal::FileSearch { index, .. })
            | Some(Modal::CodeSearch { index, .. })
            | Some(Modal::SymbolPicker { index, .. })
            | Some(Modal::FindInFile { index, .. }) => *index = clicked,
            _ => {}
        }
        return app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    let (width_percent, height) = modal_dimensions(&modal);
    let modal_area = centered_rect(width_percent, height, outer);
    if !point_in(modal_area, column, row) {
        return app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    }
    let inner_row = row.saturating_sub(modal_area.y.saturating_add(1));
    match &modal {
        Modal::Settings { .. } if inner_row < 4 => {
            if let Some(Modal::Settings { index }) = app.modal.as_mut() {
                *index = usize::from(inner_row);
            }
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        }
        Modal::AuthMenu { .. } if inner_row < 3 => {
            if let Some(Modal::AuthMenu { index }) = app.modal.as_mut() {
                *index = usize::from(inner_row);
            }
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        }
        Modal::ConfirmClearHistory if inner_row >= 2 => app.handle_key(KeyEvent::new(
            if inner_row == 2 {
                KeyCode::Enter
            } else {
                KeyCode::Esc
            },
            KeyModifiers::NONE,
        )),
        Modal::ConfirmClearCache { .. } if inner_row >= 4 => app.handle_key(KeyEvent::new(
            if inner_row == 4 {
                KeyCode::Enter
            } else {
                KeyCode::Esc
            },
            KeyModifiers::NONE,
        )),
        Modal::CacheManager { lines } => {
            let action_row = lines.len().saturating_add(1);
            let clicked = usize::from(inner_row);
            if clicked == action_row {
                app.handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
            } else if clicked == action_row + 1 {
                app.handle_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE))
            } else if clicked == action_row + 2 {
                app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            } else {
                AppCommand::None
            }
        }
        Modal::RateLimit { .. } if inner_row >= 3 => app.handle_key(KeyEvent::new(
            if inner_row == 3 {
                KeyCode::Enter
            } else {
                KeyCode::Esc
            },
            KeyModifiers::NONE,
        )),
        Modal::Help | Modal::Error { .. } => {
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        }
        Modal::TokenInput { .. } => AppCommand::None,
        _ => AppCommand::None,
    }
}

fn modal_dimensions(modal: &Modal) -> (u16, u16) {
    match modal {
        Modal::Help => (96, 50),
        Modal::ConfirmClearHistory => (62, 9),
        Modal::CacheManager { .. } => (78, 14),
        Modal::ConfirmClearCache { .. } => (68, 11),
        Modal::Settings { .. } => (68, 12),
        Modal::Error { .. } => (76, 12),
        Modal::RateLimit { .. } => (68, 10),
        Modal::AuthMenu { .. } => (86, 12),
        Modal::TokenInput { .. } => (86, 10),
        Modal::BranchPicker { .. } => (82, 26),
        Modal::RepositorySearch { .. } => (92, 27),
        Modal::FileSearch { .. } | Modal::FindInFile { .. } => (92, 28),
        Modal::CodeSearch { .. } => (96, 28),
        Modal::SymbolPicker { .. } => (88, 28),
    }
}

fn list_index_at(
    area: Rect,
    row: u16,
    start: usize,
    item_height: u16,
    len: usize,
) -> Option<usize> {
    let inner = bordered_inner(area);
    if row < inner.y || row >= inner.y.saturating_add(inner.height) || item_height == 0 {
        return None;
    }
    let offset = usize::from(row.saturating_sub(inner.y) / item_height);
    let index = start.saturating_add(offset);
    (index < len).then_some(index)
}

fn tab_index_at(area: Rect, column: u16, labels: &[&str]) -> Option<usize> {
    if column < area.x || column >= area.x.saturating_add(area.width) {
        return None;
    }
    let mut x = area.x;
    for (index, label) in labels.iter().enumerate() {
        let width = UnicodeWidthStr::width(*label).min(u16::MAX as usize) as u16;
        if column >= x && column < x.saturating_add(width) {
            return Some(index);
        }
        x = x.saturating_add(width).saturating_add(2);
    }
    None
}

fn wrapped_logical_line_at_row(
    text: &str,
    start: usize,
    clicked_row: usize,
    width: usize,
    prefix_width: usize,
) -> usize {
    let width = width.max(1);
    let mut visual_row = 0usize;
    for (offset, line) in text.lines().skip(start).enumerate() {
        let display_width = prefix_width.saturating_add(UnicodeWidthStr::width(line));
        let height = display_width.max(1).div_ceil(width);
        if clicked_row < visual_row.saturating_add(height) {
            return start.saturating_add(offset);
        }
        visual_row = visual_row.saturating_add(height);
    }
    text.lines().count().max(1).saturating_sub(1)
}

fn bordered_inner(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn point_in(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn header_title(app: &App) -> String {
    match app.screen {
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
    }
}

fn header_right_text(app: &App) -> String {
    let auth = if let Some(user) = app.auth_user.as_deref() {
        format!("GitHub @{user}")
    } else if app.authenticated {
        "GitHub authenticated".to_owned()
    } else {
        "GitHub anonymous · F2/a: authenticate".to_owned()
    };
    let rate = app.rate_limit.as_ref().and_then(|rate| {
        let remaining = rate.remaining?;
        let limit = rate.limit?;
        Some(format!(
            "{} {remaining}/{limit}",
            rate.resource.as_deref().unwrap_or("api")
        ))
    });
    rate.map_or(auth.clone(), |rate| format!("{auth} · {rate}"))
}

fn header_right_width(app: &App, width: u16) -> u16 {
    let desired = UnicodeWidthStr::width(header_right_text(app).as_str()).saturating_add(2);
    let available = if width > 10 {
        usize::from(width.saturating_sub(10))
    } else {
        usize::from(width)
    };
    desired.min(available) as u16
}

fn header_height(app: &App, width: u16) -> u16 {
    let right_width = header_right_width(app, width);
    let left_width = width.saturating_sub(right_width).max(1);
    let left_text = format!(" RepoTrek  {}", header_title(app));
    wrapped_line_count(&left_text, left_width)
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let title = header_title(app);
    let right = header_right_text(app);
    let right_width = header_right_width(app, area.width);
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
        .wrap(Wrap { trim: false })
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
    let action_layout = footer_action_layout(app, area.width);
    let actions = action_layout.lines.join("\n");
    let detail = app
        .status_text()
        .or_else(|| app.cache_status_text())
        .unwrap_or("Cache is empty or unused · F5 performs an online conditional refresh");
    let action_height = action_layout.lines.len().max(1).min(u16::MAX as usize) as u16;
    let [actions_area, detail_area] =
        Layout::vertical([Constraint::Length(action_height), Constraint::Min(1)]).areas(area);
    let detail_style = if app.status_text().is_some() {
        Style::new()
            .fg(theme.success)
            .bg(theme.background)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.muted).bg(theme.background)
    };
    frame.render_widget(
        Paragraph::new(actions).style(
            Style::new()
                .fg(theme.accent)
                .bg(theme.background)
                .add_modifier(Modifier::BOLD),
        ),
        actions_area,
    );
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: false })
            .style(detail_style),
        detail_area,
    );
}

#[derive(Debug, Clone)]
struct FooterHint {
    label: &'static str,
    event: Option<KeyEvent>,
}

impl FooterHint {
    fn key(label: &'static str, code: KeyCode) -> Self {
        Self {
            label,
            event: Some(KeyEvent::new(code, KeyModifiers::NONE)),
        }
    }

    fn modified(label: &'static str, code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self {
            label,
            event: Some(KeyEvent::new(code, modifiers)),
        }
    }

    fn info(label: &'static str) -> Self {
        Self { label, event: None }
    }
}

fn compact_footer_hints(app: &App) -> Vec<FooterHint> {
    let mut hints = match app.screen {
        Screen::Home => vec![
            FooterHint::key("[Enter] open", KeyCode::Enter),
            FooterHint::key("[Tab] section", KeyCode::Tab),
            FooterHint::key("[↑] up", KeyCode::Up),
            FooterHint::key("[↓] down", KeyCode::Down),
            FooterHint::key("[/] search", KeyCode::Char('/')),
            FooterHint::key("[F5] refresh", KeyCode::F(5)),
        ],
        Screen::Repository => {
            let mut hints = vec![
                FooterHint::key("[Tab] tabs", KeyCode::Tab),
                FooterHint::key("[↑] up", KeyCode::Up),
                FooterHint::key("[↓] down", KeyCode::Down),
                FooterHint::key("[PageUp] page", KeyCode::PageUp),
                FooterHint::key("[PageDown] page", KeyCode::PageDown),
                FooterHint::key("[Enter] open", KeyCode::Enter),
                FooterHint::key("[/] search", KeyCode::Char('/')),
                FooterHint::key("[Esc] back", KeyCode::Esc),
            ];
            if app.repository.as_ref().is_some_and(|repository| {
                matches!(
                    repository.tab,
                    RepositoryTab::PullRequests | RepositoryTab::Issues
                )
            }) {
                hints.extend([
                    FooterHint::key("[[] filter", KeyCode::Char('[')),
                    FooterHint::key("[]] filter", KeyCode::Char(']')),
                ]);
            }
            hints
        }
        Screen::File => vec![
            FooterHint::key("[Tab] view", KeyCode::Tab),
            FooterHint::key("[↑] up", KeyCode::Up),
            FooterHint::key("[↓] down", KeyCode::Down),
            FooterHint::key("[PageUp] page", KeyCode::PageUp),
            FooterHint::key("[PageDown] page", KeyCode::PageDown),
            FooterHint::key("[/] find", KeyCode::Char('/')),
            FooterHint::key("[@] symbols", KeyCode::Char('@')),
            FooterHint::key("[d] definition", KeyCode::Char('d')),
            FooterHint::key("[Esc] back", KeyCode::Esc),
        ],
        Screen::Commit => vec![
            FooterHint::key("[↑] up", KeyCode::Up),
            FooterHint::key("[↓] down", KeyCode::Down),
            FooterHint::key("[PageUp] page", KeyCode::PageUp),
            FooterHint::key("[PageDown] page", KeyCode::PageDown),
            FooterHint::key("[o] GitHub", KeyCode::Char('o')),
            FooterHint::key("[Esc] back", KeyCode::Esc),
        ],
        Screen::Detail => vec![
            FooterHint::key("[↑] up", KeyCode::Up),
            FooterHint::key("[↓] down", KeyCode::Down),
            FooterHint::key("[PageUp] page", KeyCode::PageUp),
            FooterHint::key("[PageDown] page", KeyCode::PageDown),
            FooterHint::key("[o] GitHub", KeyCode::Char('o')),
            FooterHint::key("[Esc] back", KeyCode::Esc),
        ],
    };
    hints.extend([
        FooterHint::key("[,] settings", KeyCode::Char(',')),
        FooterHint::key("[F10] all keys", KeyCode::F(10)),
        FooterHint::key("[?] help", KeyCode::F(1)),
    ]);
    hints
}

fn footer_hints(app: &App) -> Vec<FooterHint> {
    if app.settings.footer_mode == FooterMode::Compact {
        return compact_footer_hints(app);
    }
    let common = || {
        vec![
            FooterHint::key("[c/F8] cache", KeyCode::F(8)),
            FooterHint::key("[a/F2] auth", KeyCode::F(2)),
            FooterHint::key("[?/F1] help", KeyCode::F(1)),
            FooterHint::key("[,] settings", KeyCode::Char(',')),
            FooterHint::key("[T] theme", KeyCode::Char('T')),
            FooterHint::key("[F10] compact", KeyCode::F(10)),
            FooterHint::modified("[q/Ctrl+Q] quit", KeyCode::Char('q'), KeyModifiers::CONTROL),
            FooterHint::info("[Mouse] click/wheel"),
        ]
    };
    let vertical_navigation = || {
        vec![
            FooterHint::key("[↑/k] up", KeyCode::Up),
            FooterHint::key("[↓/j] down", KeyCode::Down),
            FooterHint::key("[PageUp] page up", KeyCode::PageUp),
            FooterHint::key("[PageDown] page down", KeyCode::PageDown),
            FooterHint::key("[Home/g] first", KeyCode::Home),
            FooterHint::key("[End/G] last", KeyCode::End),
        ]
    };
    let reader_selection = || {
        vec![
            FooterHint::modified(
                "[Shift+K] extend up",
                KeyCode::Char('K'),
                KeyModifiers::SHIFT,
            ),
            FooterHint::modified(
                "[Shift+J] extend down",
                KeyCode::Char('J'),
                KeyModifiers::SHIFT,
            ),
            FooterHint::key("[v] toggle selection", KeyCode::Char('v')),
            FooterHint::modified(
                "[Ctrl+A/Shift+A] select all",
                KeyCode::Char('a'),
                KeyModifiers::CONTROL,
            ),
            FooterHint::modified(
                "[y/Ctrl+C/Shift+C] copy",
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            ),
        ]
    };

    match app.screen {
        Screen::Home => {
            let mut hints = vec![
                FooterHint::key("[Enter] open/search", KeyCode::Enter),
                FooterHint::key("[Tab/→] next section", KeyCode::Tab),
                FooterHint::key("[Shift+Tab/←] previous section", KeyCode::BackTab),
                FooterHint::key("[↑/k] up", KeyCode::Up),
                FooterHint::key("[↓/j] down", KeyCode::Down),
                FooterHint::key("[PageUp] page up", KeyCode::PageUp),
                FooterHint::key("[PageDown] page down", KeyCode::PageDown),
                FooterHint::key("[Home] first", KeyCode::Home),
                FooterHint::key("[End] last", KeyCode::End),
                FooterHint::key("[/] focus search", KeyCode::Char('/')),
                FooterHint::key("[d] delete history", KeyCode::Char('d')),
                FooterHint::modified(
                    "[Ctrl+D] clear history",
                    KeyCode::Char('d'),
                    KeyModifiers::CONTROL,
                ),
                FooterHint::key("[F5/Ctrl+R/r] update", KeyCode::F(5)),
            ];
            hints.extend(common());
            hints
        }
        Screen::Repository => {
            let mut hints = vec![
                FooterHint::key("[1] Code", KeyCode::Char('1')),
                FooterHint::key("[2] Commits", KeyCode::Char('2')),
                FooterHint::key("[3] Pull requests", KeyCode::Char('3')),
                FooterHint::key("[4] Issues", KeyCode::Char('4')),
                FooterHint::key("[5] Actions", KeyCode::Char('5')),
                FooterHint::key("[6] Releases", KeyCode::Char('6')),
                FooterHint::key("[Tab/→/l] next tab", KeyCode::Tab),
                FooterHint::key("[Shift+Tab/←/h] previous tab", KeyCode::BackTab),
            ];
            if let Some(repository) = app.repository.as_ref()
                && matches!(
                    repository.tab,
                    RepositoryTab::PullRequests | RepositoryTab::Issues
                )
            {
                let state_label = match repository.tab {
                    RepositoryTab::PullRequests => repository.pull_request_filter.label(),
                    RepositoryTab::Issues => repository.issue_filter.label(),
                    RepositoryTab::Code
                    | RepositoryTab::Commits
                    | RepositoryTab::Actions
                    | RepositoryTab::Releases => "",
                };
                hints.push(FooterHint::info(match state_label {
                    "Open" => "[State] Open",
                    "Closed" => "[State] Closed",
                    "All" => "[State] All",
                    _ => "[State]",
                }));
                hints.extend([
                    FooterHint::key("[[] previous filter", KeyCode::Char('[')),
                    FooterHint::key("[]] next filter", KeyCode::Char(']')),
                    FooterHint::key("[O] open", KeyCode::Char('O')),
                    FooterHint::key("[C] closed", KeyCode::Char('C')),
                    FooterHint::key("[A] all", KeyCode::Char('A')),
                ]);
            }
            hints.extend(vertical_navigation());
            hints.extend([
                FooterHint::key("[Enter] open", KeyCode::Enter),
                FooterHint::key("[Esc/u/Backspace] parent/back", KeyCode::Esc),
                FooterHint::key("[F5/r] update", KeyCode::F(5)),
                FooterHint::key("[B] branch", KeyCode::Char('B')),
                FooterHint::key("[f] file finder", KeyCode::Char('f')),
                FooterHint::key("[s or /] code search", KeyCode::Char('s')),
                FooterHint::key("[o] GitHub", KeyCode::Char('o')),
            ]);
            hints.extend(common());
            hints
        }
        Screen::File => {
            let mut hints = vec![
                FooterHint::key("[1] Code", KeyCode::Char('1')),
                FooterHint::key("[2] Blame", KeyCode::Char('2')),
                FooterHint::key("[3] History", KeyCode::Char('3')),
                FooterHint::key("[Tab] next tab", KeyCode::Tab),
                FooterHint::key("[Shift+Tab] previous tab", KeyCode::BackTab),
            ];
            hints.extend(vertical_navigation());
            hints.extend([
                FooterHint::key("[←/h] scroll left", KeyCode::Left),
                FooterHint::key("[→/l] scroll right", KeyCode::Right),
            ]);
            hints.extend(reader_selection());
            hints.extend([
                FooterHint::key("[/ or Ctrl+F] find", KeyCode::Char('/')),
                FooterHint::key("[n] next match", KeyCode::Char('n')),
                FooterHint::modified(
                    "[N] previous match",
                    KeyCode::Char('N'),
                    KeyModifiers::SHIFT,
                ),
                FooterHint::key("[@] symbols", KeyCode::Char('@')),
                FooterHint::key("[d] definition", KeyCode::Char('d')),
                FooterHint::key("[Enter] open History/Blame item", KeyCode::Enter),
                FooterHint::key("[F5/r] update", KeyCode::F(5)),
                FooterHint::key("[p] print/export", KeyCode::Char('p')),
                FooterHint::key("[w] wrap", KeyCode::Char('w')),
                FooterHint::key("[Esc/b] back", KeyCode::Esc),
            ]);
            hints.extend(common());
            hints
        }
        Screen::Commit => {
            let mut hints = vertical_navigation();
            hints.extend([
                FooterHint::key("[←/h] scroll left", KeyCode::Left),
                FooterHint::key("[→/l] scroll right", KeyCode::Right),
            ]);
            hints.extend(reader_selection());
            hints.extend([
                FooterHint::key("[o] GitHub", KeyCode::Char('o')),
                FooterHint::key("[p] print/export", KeyCode::Char('p')),
                FooterHint::key("[w] wrap", KeyCode::Char('w')),
                FooterHint::key("[Esc/b] back", KeyCode::Esc),
            ]);
            hints.extend(common());
            hints
        }
        Screen::Detail => {
            let mut hints = vertical_navigation();
            hints.extend([
                FooterHint::key("[←/h] scroll left", KeyCode::Left),
                FooterHint::key("[→/l] scroll right", KeyCode::Right),
            ]);
            hints.extend(reader_selection());
            hints.extend([
                FooterHint::key("[o] GitHub", KeyCode::Char('o')),
                FooterHint::key("[w] wrap", KeyCode::Char('w')),
                FooterHint::key("[Esc/b] back", KeyCode::Esc),
            ]);
            hints.extend(common());
            hints
        }
    }
}

#[derive(Debug, Clone)]
struct FooterHit {
    row: u16,
    start: u16,
    end: u16,
    event: KeyEvent,
}

#[derive(Debug, Clone)]
struct FooterActionLayout {
    lines: Vec<String>,
    hits: Vec<FooterHit>,
}

fn footer_action_layout(app: &App, width: u16) -> FooterActionLayout {
    let max_width = usize::from(width.max(1));
    let mut lines = vec![String::new()];
    let mut line_widths = vec![0_usize];
    let mut hits = Vec::new();

    for hint in footer_hints(app) {
        let chunks = split_display_width(hint.label, max_width);
        for (chunk_index, chunk) in chunks.into_iter().enumerate() {
            let chunk_width = UnicodeWidthStr::width(chunk.as_str());
            let current = lines.len() - 1;
            let separator_width = if lines[current].is_empty() || chunk_index > 0 {
                0
            } else {
                2
            };
            if chunk_index > 0
                || (!lines[current].is_empty()
                    && line_widths[current] + separator_width + chunk_width > max_width)
            {
                lines.push(String::new());
                line_widths.push(0);
            }

            let current = lines.len() - 1;
            if !lines[current].is_empty() && chunk_index == 0 {
                lines[current].push_str("  ");
                line_widths[current] += 2;
            }
            let start = line_widths[current];
            lines[current].push_str(&chunk);
            line_widths[current] += chunk_width;
            if let Some(event) = hint.event {
                hits.push(FooterHit {
                    row: current.min(u16::MAX as usize) as u16,
                    start: start.min(u16::MAX as usize) as u16,
                    end: line_widths[current].min(u16::MAX as usize) as u16,
                    event,
                });
            }
        }
    }

    FooterActionLayout { lines, hits }
}

fn split_display_width(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0).max(1);
        if !current.is_empty() && current_width + character_width > max_width {
            chunks.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(character);
        current_width += character_width;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks
}

fn footer_height(app: &App, width: u16) -> u16 {
    let detail = app
        .status_text()
        .or_else(|| app.cache_status_text())
        .unwrap_or("Cache is empty or unused · F5 performs an online conditional refresh");
    (footer_action_layout(app, width).lines.len() as u16)
        .saturating_add(wrapped_line_count(detail, width))
        .max(2)
}

fn wrapped_line_count(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    text.lines()
        .map(|line| {
            let display_width = UnicodeWidthStr::width(line).max(1);
            display_width.div_ceil(width) as u16
        })
        .sum::<u16>()
        .max(1)
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
    let visible = visible_two_line_items(area);
    let start = centered_window_start(app.home.history_index, app.home.history.len(), visible);
    let items = app
        .home
        .history
        .iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, entry)| {
            history_item(
                entry,
                start + offset == app.home.history_index,
                focused,
                theme,
            )
        })
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
    let visible = visible_two_line_items(area);
    let start = centered_window_start(selected, cards.len(), visible);
    let items = cards
        .iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, card)| card_item(card, start + offset == selected, focused, app, theme))
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
        .title(Line::styled(
            title,
            Style::new()
                .fg(theme.accent)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
        ))
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
                .title(Span::styled(
                    format!("{} {path} · Esc/u/.. parent", app.icons.branch),
                    Style::new()
                        .fg(theme.accent_text)
                        .bg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )),
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
    let visible = visible_two_line_items(area);
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
                    .title(Line::styled(
                        format!("Commits · page {} · Enter: details", state.commit_page),
                        Style::new()
                            .fg(theme.accent_text)
                            .bg(theme.accent)
                            .add_modifier(Modifier::BOLD),
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
    let filter = state.pull_request_filter;

    if state.pull_requests.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    format!("No {} pull requests", filter.api_value()),
                    Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::styled(
                    format!(
                        "GitHub returned no {} pull requests on the first API page (up to 100 items). Press [ or ] to cycle, or O, C, and A to select Open, Closed, or All directly.",
                        filter.api_value()
                    ),
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
                    .title(format!("Pull requests · {}", filter.label())),
            ),
            area,
        );
        return;
    }

    let visible = visible_two_line_items(area);
    let start = centered_window_start(state.list_index, state.pull_requests.len(), visible);
    let items = state
        .pull_requests
        .iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, item)| {
            let index = start + offset;
            let draft = if item.draft { " · Draft" } else { "" };
            let (status, status_style) = if item.merged {
                ("MERGED", Style::new().fg(theme.success))
            } else if item.state.eq_ignore_ascii_case("closed") {
                ("CLOSED", Style::new().fg(theme.danger))
            } else {
                ("OPEN", Style::new().fg(theme.accent))
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{status:<6} "),
                        status_style.add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("#{}  {}", item.number, item.title),
                        Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                    ),
                ]),
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
    let title = format!(
        "Pull requests · {} · Enter: view · o: GitHub",
        filter.label()
    );
    draw_generic_list(frame, area, &title, items, theme);
}

fn draw_issues(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let state = app.repository.as_ref().expect("repository state");
    let filter = state.issue_filter;

    if state.issues.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    format!("No {} issues", filter.api_value()),
                    Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::styled(
                    format!(
                        "GitHub returned no {} issues after excluding pull requests from the first API page. Press [ or ] to cycle, or O, C, and A to select Open, Closed, or All directly.",
                        filter.api_value()
                    ),
                    Style::new().fg(theme.muted),
                ),
                Line::raw(""),
                Line::styled(
                    "Press o to open the Issues page on GitHub.",
                    Style::new().fg(theme.accent),
                ),
            ])
            .wrap(Wrap { trim: false })
            .style(Style::new().bg(theme.surface).fg(theme.text))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(theme.border))
                    .title(format!("Issues · {}", filter.label())),
            ),
            area,
        );
        return;
    }

    let visible = visible_two_line_items(area);
    let start = centered_window_start(state.list_index, state.issues.len(), visible);
    let items = state
        .issues
        .iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, item)| {
            let index = start + offset;
            let labels = if item.labels.is_empty() {
                String::new()
            } else {
                format!(" · {}", item.labels.join(", "))
            };
            let (status, status_style) = if item.state.eq_ignore_ascii_case("closed") {
                ("CLOSED", Style::new().fg(theme.danger))
            } else {
                ("OPEN", Style::new().fg(theme.accent))
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{status:<6} "),
                        status_style.add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("#{}  {}", item.number, item.title),
                        Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
                    ),
                ]),
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
    let title = format!("Issues · {} · Enter: view · o: GitHub", filter.label());
    draw_generic_list(frame, area, &title, items, theme);
}

fn draw_actions(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let state = app.repository.as_ref().expect("repository state");
    let visible = visible_two_line_items(area);
    let start = centered_window_start(state.list_index, state.workflow_runs.len(), visible);
    let items = state
        .workflow_runs
        .iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, item)| {
            let index = start + offset;
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
    let visible = visible_two_line_items(area);
    let start = centered_window_start(state.list_index, state.releases.len(), visible);
    let items = state
        .releases
        .iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, item)| {
            let index = start + offset;
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
                .title(Span::styled(
                    format!(" {title} "),
                    Style::new()
                        .fg(theme.accent_text)
                        .bg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )),
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
        Span::styled(
            format!(
                "{} · lines {}-{} selected · wrap {}",
                file.path,
                start + 1,
                end + 1,
                on_off(app.settings.wrap_code)
            ),
            Style::new()
                .fg(theme.accent_text)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            format!(
                "{} · line {} / {} · wrap {}",
                file.path,
                file.cursor_line + 1,
                total,
                on_off(app.settings.wrap_code)
            ),
            Style::new()
                .fg(theme.accent_text)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
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
    let language = detect_language(&file.path, &file.content);
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
                source_spans_with_language(line, language, theme)
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
    let language = detect_language(&file.path, &file.content);
    file.content
        .lines()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, line)| {
            let line_index = start + offset;
            let bg = line_background(file, line_index, selection_start, selection_end, theme);
            let spans = source_spans_with_language(line, language, theme)
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
    let visible = visible_two_line_items(area);
    let start = centered_window_start(file.history_index, file.history.len(), visible);
    let items = file
        .history
        .iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(|(offset, commit)| {
            let index = start + offset;
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
                .title(Span::styled(
                    format!("History · {} · Enter: commit", file.path),
                    Style::new()
                        .fg(theme.accent_text)
                        .bg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )),
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
            Span::styled(
                format!(
                    "{title} · line {} / {total} · wrap {}",
                    cursor_line + 1,
                    on_off(wrap)
                ),
                Style::new()
                    .fg(theme.accent_text)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
        },
        |(start, end)| {
            Span::styled(
                format!(
                    "{title} · lines {}-{} selected · wrap {}",
                    start + 1,
                    end + 1,
                    on_off(wrap)
                ),
                Style::new()
                    .fg(theme.accent_text)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
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
        let bg = if selected {
            theme.selection
        } else if line_index == cursor_line {
            theme.cursor
        } else {
            theme.diff_hunk_bg
        };
        return Line::styled(line.to_owned(), Style::new().fg(theme.accent).bg(bg));
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
        } else if line_index == cursor_line {
            theme.cursor
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
        Modal::Help => draw_text_modal(frame, "Help", help_lines(app, theme), 96, 50, theme),
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
        Modal::CacheManager { lines } => {
            let mut content = lines.iter().cloned().map(Line::raw).collect::<Vec<_>>();
            content.push(Line::raw(""));
            content.push(Line::styled(
                "d / Delete  Clear all cache",
                Style::new().fg(theme.danger),
            ));
            content.push(Line::styled(
                "F5 / r      Recalculate summary",
                Style::new().fg(theme.accent),
            ));
            content.push(Line::styled(
                "Esc          Close",
                Style::new().fg(theme.muted),
            ));
            draw_text_modal(frame, "Cache manager", content, 78, 14, theme);
        }
        Modal::ConfirmClearCache { lines } => {
            let detail = lines.first().map_or("", String::as_str);
            draw_text_modal(
                frame,
                "Clear all RepoTrek cache?",
                vec![
                    Line::raw(detail.to_owned()),
                    Line::raw("Cached API responses and source files will be removed."),
                    Line::raw("Browsing history and settings are not affected."),
                    Line::raw(""),
                    Line::styled("Enter / y  Clear cache", Style::new().fg(theme.danger)),
                    Line::styled("Esc / n    Go back", Style::new().fg(theme.muted)),
                ],
                68,
                11,
                theme,
            );
        }
        Modal::Settings { index } => {
            let options = [
                format!("Theme               {}", app.settings.theme.label()),
                format!("Source wrapping      {}", on_off(app.settings.wrap_code)),
                format!("Diff/detail wrapping {}", on_off(app.settings.wrap_diff)),
                format!("Footer key hints     {}", app.settings.footer_mode.label()),
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
            draw_text_modal(frame, "Settings", lines, 68, 12, theme);
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
            let mut lines = filtered
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
            if lines.is_empty() {
                let message = if query.is_empty() {
                    "No branches or tags are available".to_owned()
                } else {
                    format!("No branches or tags match `{query}`")
                };
                lines.push(Line::styled(message, Style::new().fg(theme.muted)));
            }
            draw_palette(frame, "Switch branch", query, lines, 82, 26, theme);
        }
        Modal::RepositorySearch {
            query,
            results,
            index,
        } => {
            let visible = 20;
            let start = centered_window_start(*index, results.len(), visible);
            let mut lines = results
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
                .collect::<Vec<_>>();
            if lines.is_empty() {
                let message = if query.is_empty() {
                    "Type a repository name or topic, then press Enter".to_owned()
                } else {
                    format!("No repositories matched `{query}` · try broader terms")
                };
                lines.push(Line::styled(message, Style::new().fg(theme.muted)));
            }
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
            let mut lines = results
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
                .collect::<Vec<_>>();
            if lines.is_empty() {
                let message = if query.is_empty() {
                    "No files are available in the repository tree".to_owned()
                } else {
                    format!("No file paths match `{query}`")
                };
                lines.push(Line::styled(message, Style::new().fg(theme.muted)));
            }
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
            let mut lines = results
                .iter()
                .skip(start)
                .take(visible)
                .enumerate()
                .map(|(offset, result)| {
                    Line::styled(
                        format_code_search_result(result),
                        palette_item_style(start + offset == *index, theme),
                    )
                })
                .collect::<Vec<_>>();
            if lines.is_empty() {
                let message = if query.is_empty() {
                    if *mode == CodeSearchMode::Definition {
                        "Type a symbol name, then press Enter".to_owned()
                    } else {
                        "Type source text, then press Enter".to_owned()
                    }
                } else if *mode == CodeSearchMode::Definition {
                    format!("No definition found in the source files scanned for `{query}`")
                } else {
                    format!("No match found in the source files scanned for `{query}`")
                };
                lines.push(Line::styled(message, Style::new().fg(theme.muted)));
            }
            draw_palette(frame, title, query, lines, 96, 28, theme);
        }
        Modal::SymbolPicker {
            query,
            all_symbols,
            results,
            index,
        } => {
            let visible = 22;
            let start = centered_window_start(*index, results.len(), visible);
            let mut lines = results
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
                .collect::<Vec<_>>();
            let language = app.file.as_ref().map_or("Unknown", |file| {
                detect_language(&file.path, &file.content).label()
            });
            if lines.is_empty() {
                let message = if all_symbols.is_empty() {
                    format!("No outline symbols were detected for {language}")
                } else {
                    format!("No symbols match `{query}`")
                };
                lines.push(Line::styled(message, Style::new().fg(theme.muted)));
            }
            let title = format!(
                "Outline / symbols · {language} · {}/{} items",
                results.len(),
                all_symbols.len()
            );
            draw_palette(frame, &title, query, lines, 88, 28, theme);
        }
        Modal::FindInFile {
            query,
            matches,
            index,
        } => {
            let visible = 22;
            let start = centered_window_start(*index, matches.len(), visible);
            let content_lines = app
                .file
                .as_ref()
                .map(|file| file.content.lines().collect::<Vec<_>>())
                .unwrap_or_default();
            let mut lines = matches
                .iter()
                .skip(start)
                .take(visible)
                .enumerate()
                .map(|(offset, line)| {
                    let source = content_lines.get(*line).copied().unwrap_or_default().trim();
                    Line::styled(
                        format!("{:>5}  {}", line + 1, source),
                        palette_item_style(start + offset == *index, theme),
                    )
                })
                .collect::<Vec<_>>();
            if lines.is_empty() {
                let message = if query.is_empty() {
                    "Type text to find in the current file".to_owned()
                } else {
                    format!("No matches for `{query}` in the current file")
                };
                lines.push(Line::styled(message, Style::new().fg(theme.muted)));
            }
            draw_palette(frame, "Find in current file", query, lines, 92, 28, theme);
        }
    }
}

fn format_code_search_result(result: &crate::model::CodeSearchResult) -> String {
    let location = result.line.map_or_else(
        || result.path.clone(),
        |line| format!("{}:{line}", result.path),
    );
    let kind = result
        .kind
        .as_deref()
        .map(|kind| format!(" [{kind}]"))
        .unwrap_or_default();
    let preview = result
        .preview
        .as_deref()
        .filter(|preview| !preview.is_empty())
        .map(|preview| format!("  {preview}"))
        .unwrap_or_default();
    format!("{location}{kind}{preview}")
}

fn palette_item_style(selected: bool, theme: Theme) -> Style {
    if selected {
        Style::new().bg(theme.selection).fg(theme.text)
    } else {
        Style::new().bg(theme.surface).fg(theme.text)
    }
}

const PALETTE_INSTRUCTION: &str = "Enter search/open · ↑↓/Tab select · PageUp/PageDown page · Home/End · type to refine · Ctrl+V paste · Esc close";

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
    let area = centered_rect(width_percent, height, frame.area());
    let inner_width = usize::from(area.width.saturating_sub(2).max(1));
    let mut content = styled_wrapped_lines(
        &format!("> {query}"),
        Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        inner_width,
    );
    content.extend(styled_wrapped_lines(
        PALETTE_INSTRUCTION,
        Style::new().fg(theme.muted),
        inner_width,
    ));
    content.push(Line::raw(""));
    if lines.is_empty() {
        lines.push(Line::styled(
            "No matches yet · type a query, then press Enter",
            Style::new().fg(theme.muted),
        ));
    }
    content.extend(lines);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(content)
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

fn styled_wrapped_lines(text: &str, style: Style, max_width: usize) -> Vec<Line<'static>> {
    split_display_width(text, max_width)
        .into_iter()
        .map(|line| Line::styled(line, style))
        .collect()
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
    let area = centered_rect(72, 7, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!("●  {message}"))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
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
        Line::raw("  owner/repo          Open that exact repository directly"),
        Line::raw("  other text          Search repositories using GitHub best match"),
        Line::raw("  Enter               Open the selected item or submit search"),
        Line::raw("  Tab / Shift+Tab     Move between Search, History, Featured, Recommended"),
        Line::raw("  ↑↓ / j k            Move within the focused list"),
        Line::raw("  PageUp/PageDown     Move by one page"),
        Line::raw("  Home/End            First/last item"),
        Line::raw("  d / Ctrl+D          Delete one/all History entries"),
        Line::raw("  F5 / Ctrl+R / r     Force online refresh with ETag validation"),
        Line::raw(""),
        Line::styled(
            "Repository",
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::raw("  1..6 / Tab          Select Code, Commits, PRs, Issues, Actions, Releases"),
        Line::raw("  ↑↓ / j k            Move selection; PageUp/PageDown and Home/End also work"),
        Line::raw("  Enter               Open the selected directory, file, commit, or item"),
        Line::raw("  Esc / u / Backspace Go to parent directory or previous screen"),
        Line::raw("  B                   Switch branch"),
        Line::raw("  f                   Recursive file finder"),
        Line::raw("  s or /              Search text across the repository; Esc/Ctrl+C cancels"),
        Line::raw("  [ and ], O/C/A      PR/Issue state: previous/next or Open/Closed/All"),
        Line::raw("  F5 / r              Force-refresh the current repository view"),
        Line::raw("  o                   Open the selected item on GitHub"),
        Line::raw(""),
        Line::styled(
            "File reader",
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::raw("  1..3 / Tab          Code, Blame, History"),
        Line::raw("  ↑↓ / j k            Move by line; PageUp/PageDown and Home/End also work"),
        Line::raw("  ←→ / h l            Horizontal scrolling when wrapping is off"),
        Line::raw("  / or Ctrl+F         Find text in the current file"),
        Line::raw("  n / N               Next/previous current-file match"),
        Line::raw("  @                   Outline/symbol list for the detected language"),
        Line::raw("  d                   Find a local/repository definition; Esc/Ctrl+C cancels"),
        Line::raw("  F5 / r              Revalidate and reload the current file"),
        Line::raw("  Shift+J/K or v      Extend/toggle line selection"),
        Line::raw("  Ctrl+A / Shift+A    Select all lines"),
        Line::raw("  y / Ctrl+C / Shift+C Copy selected lines"),
        Line::raw("  w / p               Toggle wrapping / export print-ready HTML"),
        Line::raw(""),
        Line::styled(
            "Mouse and palettes",
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::raw(
            "  Left click          Select rows, tabs, code lines, modal results, and footer actions",
        ),
        Line::raw("  Click selected row  Open/activate it"),
        Line::raw("  Shift+click         Extend a reader line selection"),
        Line::raw("  Wheel               Move/scroll; right click goes back or closes a modal"),
        Line::raw("  Palette keys        ↑↓/Tab, PageUp/PageDown, Home/End, Enter, Ctrl+V, Esc"),
        Line::raw(""),
        Line::styled(
            "Detection, cache, and global keys",
            Style::new().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Line::raw(
            "  Language detection  Special filename, extension, shebang, then content heuristics",
        ),
        Line::raw(
            "  Special names       Makefile, Dockerfile.*, CMakeLists.txt, Gemfile, Jenkinsfile, BUILD",
        ),
        Line::raw(
            "  c / F8              Cache manager; d/Delete there clears all cache after confirmation",
        ),
        Line::raw("  F2 / a              GitHub authentication"),
        Line::raw("  , / T / ?           Settings / theme / help"),
        Line::raw("  F10                 Toggle compact/full footer key hints"),
        Line::raw("  q / Ctrl+Q          Quit"),
        Line::raw(""),
        Line::styled(
            format!(
                "Theme: {} · Source wrap: {} · Diff wrap: {} · Footer: {} · Emoji: {}",
                app.settings.theme.label(),
                on_off(app.settings.wrap_code),
                on_off(app.settings.wrap_diff),
                app.settings.footer_mode.label(),
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

fn visible_two_line_items(area: Rect) -> usize {
    (usize::from(area.height.saturating_sub(2)) / 2).max(1)
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

#[cfg(test)]
mod tests {
    use super::{split_display_width, wrapped_logical_line_at_row};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn footer_chunks_never_exceed_the_terminal_width() {
        let chunks = split_display_width("[PageDown] page down", 8);
        assert!(
            chunks
                .iter()
                .all(|chunk| UnicodeWidthStr::width(chunk.as_str()) <= 8)
        );
        assert_eq!(chunks.concat(), "[PageDown] page down");
    }

    #[test]
    fn wrapped_mouse_rows_map_back_to_the_logical_source_line() {
        let text = "short\n0123456789abcdef\nlast";
        assert_eq!(wrapped_logical_line_at_row(text, 0, 0, 8, 0), 0);
        assert_eq!(wrapped_logical_line_at_row(text, 0, 1, 8, 0), 1);
        assert_eq!(wrapped_logical_line_at_row(text, 0, 2, 8, 0), 1);
        assert_eq!(wrapped_logical_line_at_row(text, 0, 3, 8, 0), 2);
    }
}
