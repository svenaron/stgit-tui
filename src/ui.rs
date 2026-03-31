use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{App, AppMode, LineItem};
use crate::stgit::{FileStatus, PatchStatus};

pub fn draw(f: &mut Frame, app: &App) {
    match &app.mode {
        AppMode::Normal => draw_normal(f, app),
        AppMode::DiffView {
            lines,
            scroll,
            title,
        } => draw_diff(f, lines, *scroll, title),
        AppMode::Input {
            prompt,
            value,
            action: _,
        } => {
            draw_normal(f, app);
            draw_input_overlay(f, prompt, value);
        }
        AppMode::Help => draw_help(f),
        AppMode::BranchList { branches, selected } => draw_branch_list(f, branches, *selected),
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

fn draw_diff(f: &mut Frame, lines: &[String], scroll: usize, title: &str) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let main_area = chunks[0];
    let status_area = chunks[1];

    let text_lines: Vec<Line> = lines
        .iter()
        .map(|l| {
            let style = if l.starts_with('+') && !l.starts_with("+++") {
                Style::default().fg(Color::Green)
            } else if l.starts_with('-') && !l.starts_with("---") {
                Style::default().fg(Color::Red)
            } else if l.starts_with("@@") {
                Style::default().fg(Color::Cyan)
            } else if l.starts_with("diff ") || l.starts_with("---") || l.starts_with("+++") {
                Style::default().fg(Color::White).bold()
            } else {
                Style::default()
            };
            Line::from(Span::styled(l.clone(), style))
        })
        .collect();

    let paragraph = Paragraph::new(Text::from(text_lines))
        .scroll((scroll as u16, 0))
        .block(Block::default().borders(Borders::NONE));

    f.render_widget(paragraph, main_area);

    let status = Paragraph::new(Line::from(vec![
        Span::styled(title, Style::default().fg(Color::Cyan).bold()),
        Span::styled("  q:close j/k:scroll", Style::default().fg(Color::DarkGray)),
    ]))
    .style(Style::default().bg(Color::DarkGray));

    f.render_widget(status, status_area);
}

fn draw_input_overlay(f: &mut Frame, prompt: &str, value: &str) {
    let area = f.area();

    // Overwrite just the status bar with the input prompt
    let status_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };

    let input_line = Paragraph::new(Line::from(vec![
        Span::styled(prompt, Style::default().fg(Color::Cyan).bold()),
        Span::styled(value, Style::default().fg(Color::White)),
        Span::styled("_", Style::default().fg(Color::White)),
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
        "    b               Switch branch",
        "    B               Rebase onto upstream",
        "    f               Git fetch",
        "    p               Git push (with confirmation)",
        "    F               Force push (with confirmation)",
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

fn draw_branch_list(f: &mut Frame, branches: &[String], selected: usize) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let mut text_lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "  Switch Branch",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(""),
    ];

    for (i, branch) in branches.iter().enumerate() {
        let is_selected = i == selected;
        let prefix = if is_selected { "  > " } else { "    " };
        let style = if is_selected {
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(40, 40, 60))
                .bold()
        } else {
            Style::default().fg(Color::White)
        };
        text_lines.push(Line::from(Span::styled(format!("{prefix}{branch}"), style)));
    }

    let paragraph =
        Paragraph::new(Text::from(text_lines)).block(Block::default().borders(Borders::NONE));

    f.render_widget(paragraph, chunks[0]);

    let status = Paragraph::new(Line::from(vec![Span::styled(
        "  Enter:switch  n:new branch  q:cancel",
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
        line = line.patch_style(Style::default().bg(Color::Rgb(40, 40, 60)));
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
