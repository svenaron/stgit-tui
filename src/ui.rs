use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{App, AppMode, DiffSource, DiffViewState, LineItem};
use crate::stgit::{FileStatus, PatchStatus};

pub fn draw(f: &mut Frame, app: &App) {
    match &app.mode {
        AppMode::Normal => draw_normal(f, app),
        AppMode::DiffView(dv) => draw_diff(f, dv),
        AppMode::Input {
            prompt,
            value,
            completions,
            filter_text,
            ..
        } => {
            draw_normal(f, app);
            let query = filter_text.as_deref().unwrap_or(value);
            draw_input_overlay(f, prompt, value, completions, query);
        }
        AppMode::Help => draw_help(f),
    }
}

fn draw_normal(f: &mut Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let main_area = chunks[0];
    let status_area = chunks[1];

    let mut text_lines: Vec<Line> = Vec::new();
    for (line_idx, item) in app.lines.iter().enumerate() {
        let is_cursor = line_idx == app.cursor;
        let line = render_line(app, item, is_cursor);
        text_lines.push(line);
    }

    let visible_height = main_area.height as usize;
    let scroll_offset = if app.cursor >= visible_height {
        app.cursor - visible_height + 1
    } else {
        0
    };

    let paragraph = Paragraph::new(Text::from(text_lines))
        .scroll((scroll_offset as u16, 0))
        .block(Block::default().borders(Borders::NONE));

    f.render_widget(paragraph, main_area);

    let status = Paragraph::new(Line::from(vec![Span::styled(
        &app.status_msg,
        Style::default().fg(Color::Yellow),
    )]))
    .style(Style::default().bg(Color::DarkGray));

    f.render_widget(status, status_area);
}

fn draw_diff(f: &mut Frame, dv: &DiffViewState) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let main_area = chunks[0];
    let status_area = chunks[1];
    let visible_height = main_area.height as usize;

    // Adjust scroll to keep cursor visible
    let scroll = if dv.cursor >= dv.scroll + visible_height {
        dv.cursor - visible_height + 1
    } else if dv.cursor < dv.scroll {
        dv.cursor
    } else {
        dv.scroll
    };

    let selection = dv.selection_range();
    let current_hunk = dv.current_hunk_index();

    let text_lines: Vec<Line> = dv
        .lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let is_cursor = i == dv.cursor;
            let in_selection = selection.is_some_and(|(s, e)| i >= s && i <= e);
            let in_current_hunk = current_hunk
                .and_then(|hi| dv.hunks.get(hi))
                .is_some_and(|h| i >= h.start_line && i < h.end_line);

            let mut style = diff_line_style(l);

            // Highlight current hunk with a subtle background
            if in_current_hunk && !is_cursor && !in_selection {
                style = style.bg(Color::Rgb(25, 25, 35));
            }

            // Selection highlight
            if in_selection && !is_cursor {
                style = style.bg(Color::Rgb(50, 50, 80));
            }

            // Cursor
            if is_cursor {
                style = style.add_modifier(Modifier::REVERSED);
            }

            Line::from(Span::styled(l.clone(), style))
        })
        .collect();

    let paragraph = Paragraph::new(Text::from(text_lines))
        .scroll((scroll as u16, 0))
        .block(Block::default().borders(Borders::NONE));

    f.render_widget(paragraph, main_area);

    // Status bar with context-sensitive hints
    let can_stage = !matches!(dv.source, DiffSource::Patch { .. });
    let hints = if can_stage {
        if dv.selection_anchor.is_some() {
            "  q:close n/p:hunk j/k:move v:end-sel s:stage u:unstage r:revert"
        } else {
            "  q:close n/p:hunk j/k:move v:select s:stage u:unstage r:revert"
        }
    } else {
        "  q:close n/p:hunk j/k:scroll"
    };

    let hunk_info = current_hunk
        .map(|i| format!(" [{}/{}]", i + 1, dv.hunks.len()))
        .unwrap_or_default();

    let status = Paragraph::new(Line::from(vec![
        Span::styled(&dv.title, Style::default().fg(Color::Cyan).bold()),
        Span::styled(hunk_info, Style::default().fg(Color::Yellow)),
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
    ]))
    .style(Style::default().bg(Color::DarkGray));

    f.render_widget(status, status_area);
}

fn diff_line_style(line: &str) -> Style {
    if line.starts_with('+') && !line.starts_with("+++") {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Cyan)
    } else if line.starts_with("diff ") || line.starts_with("---") || line.starts_with("+++") {
        Style::default().fg(Color::White).bold()
    } else {
        Style::default()
    }
}

fn draw_input_overlay(
    f: &mut Frame,
    prompt: &str,
    value: &str,
    completions: &[String],
    query: &str,
) {
    let area = f.area();

    // Filter completions that match the query (what the user typed, not the completed value)
    let filtered: Vec<&String> = if completions.is_empty() || query.is_empty() {
        Vec::new()
    } else {
        let ql = query.to_lowercase();
        completions
            .iter()
            .filter(|c| {
                let cl = c.to_lowercase();
                cl.contains(&ql) && cl != value.to_lowercase()
            })
            .take(5)
            .collect()
    };

    let suggestion_height = filtered.len() as u16;
    let total_height = 1 + suggestion_height;
    let start_y = area.y + area.height.saturating_sub(total_height);

    // Draw suggestion lines above the input
    for (i, suggestion) in filtered.iter().enumerate() {
        let suggestion_area = Rect {
            x: area.x,
            y: start_y + i as u16,
            width: area.width,
            height: 1,
        };
        let line = Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled((*suggestion).clone(), Style::default().fg(Color::DarkGray)),
        ]))
        .style(Style::default().bg(Color::Rgb(30, 30, 40)));
        f.render_widget(line, suggestion_area);
    }

    // Input line at the bottom
    let status_area = Rect {
        x: area.x,
        y: start_y + suggestion_height,
        width: area.width,
        height: 1,
    };

    let tab_hint = if !completions.is_empty() {
        Span::styled("  [Tab: complete]", Style::default().fg(Color::DarkGray))
    } else {
        Span::default()
    };

    let input_line = Paragraph::new(Line::from(vec![
        Span::styled(prompt, Style::default().fg(Color::Cyan).bold()),
        Span::styled(value, Style::default().fg(Color::White)),
        Span::styled("_", Style::default().fg(Color::White)),
        tab_hint,
    ]))
    .style(Style::default().bg(Color::DarkGray));

    f.render_widget(input_line, status_area);
}

fn draw_help(f: &mut Frame) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let help_text = vec![
        "",
        "  stg-tui keybindings",
        "  ───────────────────",
        "",
        "  Navigation",
        "    j/k, ↑/↓       Move cursor",
        "    PgUp/PgDn       Scroll by page",
        "    Home/End        Jump to top/bottom",
        "    Enter           Expand/collapse patch files",
        "",
        "  Patch Operations",
        "    r               Refresh current patch",
        "    Ctrl-r          Refresh patch under cursor",
        "    G               Goto patch under cursor",
        "    > / <           Push next / pop current",
        "    P               Push/pop marked patches",
        "    M               Move marked patches to cursor",
        "    N               New empty patch (prompts for message)",
        "    c               Create patch from changes",
        "    e               Edit patch commit message ($EDITOR)",
        "    D               Delete patch(es)",
        "    S               Squash marked patches",
        "    C               Commit patch / uncommit history",
        "",
        "  Marking",
        "    m               Mark patch",
        "    u               Unmark patch",
        "",
        "  File Operations",
        "    i               Stage/unstage file",
        "    U               Revert file",
        "    R               Resolve conflict",
        "",
        "  Branch & Remote",
        "    b               Switch branch (Tab to complete)",
        "    B               Rebase onto upstream",
        "    f               Git fetch",
        "    p               Git push (with confirmation)",
        "    F               Force push (with confirmation)",
        "",
        "  Diff View (from = key)",
        "    n/p             Next/previous hunk",
        "    v               Start/end line selection",
        "    s               Stage hunk or selection",
        "    u               Unstage hunk or selection",
        "    r               Revert hunk or selection",
        "    q               Close diff",
        "",
        "  View & Settings",
        "    =               Show diff",
        "    t               Toggle untracked files",
        "    H               Set history size",
        "    g               Reload",
        "    ?/h             This help screen",
        "",
        "  Other",
        "    !               Repair stgit state",
        "    Ctrl-z          Undo",
        "    Ctrl-y          Redo",
        "    q               Quit",
    ];

    let lines: Vec<Line> = help_text
        .iter()
        .map(|l| Line::from(Span::styled(*l, Style::default().fg(Color::White))))
        .collect();

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::NONE));

    f.render_widget(paragraph, chunks[0]);

    let status = Paragraph::new(Line::from(vec![Span::styled(
        "  Press q or ? to close",
        Style::default().fg(Color::DarkGray),
    )]))
    .style(Style::default().bg(Color::DarkGray));

    f.render_widget(status, chunks[1]);
}

fn render_line<'a>(app: &App, item: &LineItem, is_cursor: bool) -> Line<'a> {
    let mut spans = Vec::new();

    match item {
        LineItem::Header => {
            spans.push(Span::styled(
                "Branch: ",
                Style::default().fg(Color::White).bold(),
            ));
            spans.push(Span::styled(
                app.state.branch.name.clone(),
                Style::default().fg(Color::Cyan).bold(),
            ));
            if let Some(ref upstream) = app.state.branch.upstream {
                spans.push(Span::styled(" <-> ", Style::default().fg(Color::DarkGray)));
                spans.push(Span::styled(
                    upstream.clone(),
                    Style::default().fg(Color::Cyan),
                ));
            }
        }
        LineItem::History(i) => {
            spans.push(Span::styled("  ", Style::default()));
            let subject = app.state.history.get(*i).cloned().unwrap_or_default();
            spans.push(Span::styled(subject, Style::default().fg(Color::DarkGray)));
        }
        LineItem::Patch(i) => {
            let patch = &app.state.patches[*i];
            let is_marked = app.marked.contains(i);

            let prefix = match patch.status {
                PatchStatus::Current => ">",
                PatchStatus::Applied => "+",
                PatchStatus::Unapplied => "-",
            };
            let mark = if is_marked { "*" } else { " " };
            let empty_indicator = if patch.empty { "0" } else { " " };

            let color = match patch.status {
                PatchStatus::Current => Color::White,
                PatchStatus::Applied => Color::Green,
                PatchStatus::Unapplied => Color::Red,
            };

            let mut style = Style::default().fg(color);
            if patch.status == PatchStatus::Current {
                style = style.bold();
            }

            spans.push(Span::styled(
                format!("{prefix}{mark}{empty_indicator}"),
                style,
            ));
            spans.push(Span::styled(patch.name.to_string(), style));
            if !patch.description.is_empty() {
                spans.push(Span::styled(
                    format!("  # {}", patch.description),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
        LineItem::PatchFile(pi, fi) => {
            if let Some(files) = app.patch_files.get(pi) {
                if let Some(file) = files.get(*fi) {
                    spans.push(Span::styled("      ", Style::default()));
                    spans.push(Span::styled(
                        format!("{:<12}", file.status.label()),
                        file_status_style(&file.status),
                    ));
                    spans.push(Span::styled(
                        file.path.clone(),
                        Style::default().fg(Color::White),
                    ));
                }
            }
        }
        LineItem::IndexHeader => {
            spans.push(Span::styled(
                "  Index",
                Style::default().fg(Color::Yellow).bold(),
            ));
        }
        LineItem::IndexFile(i) => {
            let file = &app.state.index_files[*i];
            spans.push(Span::styled("    ", Style::default()));
            spans.push(Span::styled(
                format!("{:<12}", file.status.label()),
                file_status_style(&file.status),
            ));
            spans.push(Span::styled(
                file.path.clone(),
                Style::default().fg(Color::White),
            ));
        }
        LineItem::WorkTreeHeader => {
            spans.push(Span::styled(
                "  Work Tree",
                Style::default().fg(Color::Yellow).bold(),
            ));
        }
        LineItem::WorkTreeFile(i) => {
            let file = &app.state.worktree_files[*i];
            spans.push(Span::styled("    ", Style::default()));
            spans.push(Span::styled(
                format!("{:<12}", file.status.label()),
                file_status_style(&file.status),
            ));
            spans.push(Span::styled(
                file.path.clone(),
                Style::default().fg(Color::White),
            ));
        }
        LineItem::Footer => {
            spans.push(Span::styled("--", Style::default().fg(Color::DarkGray)));
        }
    }

    let mut line = Line::from(spans);
    if is_cursor {
        line = line.patch_style(Style::default().add_modifier(Modifier::REVERSED));
    }
    line
}

fn file_status_style(status: &FileStatus) -> Style {
    match status {
        FileStatus::Modified => Style::default().fg(Color::Yellow),
        FileStatus::Added => Style::default().fg(Color::Green),
        FileStatus::Deleted => Style::default().fg(Color::Red),
        FileStatus::Renamed => Style::default().fg(Color::Cyan),
        FileStatus::Copied => Style::default().fg(Color::Cyan),
        FileStatus::Untracked => Style::default().fg(Color::DarkGray),
        FileStatus::Unresolved => Style::default().fg(Color::Magenta).bold(),
    }
}
