use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, LineItem};
use crate::stgit::{FileStatus, PatchStatus};

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    // Main area + status line
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let main_area = chunks[0];
    let status_area = chunks[1];

    // Build styled lines
    let mut text_lines: Vec<Line> = Vec::new();

    for (line_idx, item) in app.lines.iter().enumerate() {
        let is_cursor = line_idx == app.cursor;
        let line = render_line(app, item, is_cursor);
        text_lines.push(line);
    }

    // Calculate scroll offset to keep cursor visible
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

    // Status line
    let status = Paragraph::new(Line::from(vec![Span::styled(
        &app.status_msg,
        Style::default().fg(Color::Yellow),
    )]))
    .style(Style::default().bg(Color::DarkGray));

    f.render_widget(status, status_area);
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
            let files = app.patch_files.get(pi);
            if let Some(files) = files {
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
