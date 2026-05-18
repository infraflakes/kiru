use super::*;
use crate::colors;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

fn colorize_output(line: &str) -> Line<'static> {
    let trimmed = line.trim_start();

    // function header like "build(todo)" — color whole line
    if !trimmed.contains(' ') && trimmed.contains('(') && trimmed.ends_with(')') {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(colors::EXEC),
        ));
    }

    // execution primitives — color the keyword
    let (prefix, kw_color) = if trimmed.starts_with("log  ") {
        ("log  ", colors::LOG)
    } else if trimmed.starts_with("exec ") {
        ("exec ", colors::EXEC)
    } else if trimmed.starts_with("cd   ") {
        ("cd   ", colors::CD)
    } else if trimmed.starts_with("env  ") {
        ("env  ", colors::ENV)
    } else {
        return Line::from(line.to_string());
    };

    let indent = line.len() - trimmed.len();
    let mut spans = Vec::new();
    if indent > 0 {
        spans.push(Span::raw(line[..indent].to_string()));
    }
    spans.push(Span::styled(
        prefix.to_string(),
        Style::default().fg(kw_color),
    ));
    spans.push(Span::styled(
        trimmed[prefix.len()..].to_string(),
        Style::default().fg(colors::TEXT),
    ));
    Line::from(spans)
}

pub fn render(f: &mut Frame, model: &Model, spinner_idx: usize) {
    let size = f.size();

    // Header with badge and info
    let badge_color = match model.model_type.as_str() {
        "seq" => colors::SEQ,
        "par" => colors::PAR,
        "sync" => colors::SYNC,
        _ => colors::TEXT_BRIGHT,
    };

    let header_spans = vec![
        Span::styled(
            format!(" {} ", model.model_type),
            Style::default().fg(Color::Black).bg(badge_color),
        ),
        Span::styled(
            format!(" {} ", model.name),
            Style::default().fg(colors::TEXT),
        ),
        Span::styled(
            format!(" {} tasks ", model.tasks.len()),
            Style::default().fg(colors::MUTED),
        ),
    ];

    let header = Paragraph::new(Line::from(header_spans));
    f.render_widget(header, Rect::new(0, 0, size.width, 1));

    // Separator
    let separator =
        Paragraph::new("─".repeat(size.width as usize)).style(Style::default().fg(colors::MUTED));
    f.render_widget(separator, Rect::new(0, 1, size.width, 1));

    // Task rows with accordion expansion
    let mut y = 0;
    let max_y = size
        .height
        .saturating_sub(HEADER_HEIGHT as u16 + FOOTER_HEIGHT as u16) as usize;

    for (i, task) in model.tasks.iter().enumerate() {
        let task_row = y;
        if task_row >= model.scroll_row {
            if y - model.scroll_row >= max_y {
                break;
            }

            // Task row
            let spinner = match task.status {
                TaskStatus::Pending => "·".to_string(),
                TaskStatus::Running => SPINNER_FRAMES[spinner_idx].to_string(),
                TaskStatus::Success => "✓".to_string(),
                TaskStatus::Error => "✗".to_string(),
            };

            let color = match task.status {
                TaskStatus::Pending => colors::PENDING,
                TaskStatus::Running => colors::RUNNING,
                TaskStatus::Success => colors::OK,
                TaskStatus::Error => colors::FAILED,
            };

            let is_selected = i == model.selected;
            let style = if is_selected {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };

            let arrow = if task.status != TaskStatus::Pending {
                if task.expanded { "▼" } else { "▶" }
            } else {
                ""
            };

            let task_line = format!(
                "{} {} {} {}",
                if is_selected { "▸" } else { " " },
                spinner,
                task.name,
                arrow
            );

            let task_paragraph = Paragraph::new(task_line).style(style);
            f.render_widget(
                task_paragraph,
                Rect::new(
                    0,
                    (y - model.scroll_row) as u16 + HEADER_HEIGHT as u16,
                    size.width,
                    1,
                ),
            );
            y += 1;

            // Expanded output panel - capped at MAX_PANEL_HEIGHT, shows pruned indicator
            if task.expanded && !task.output.is_empty() {
                let total_lines = task.output.len();
                let mut panel_height = total_lines.min(MAX_PANEL_HEIGHT);
                panel_height = panel_height.min(max_y.saturating_sub(y - model.scroll_row));
                let pruned_count = total_lines.saturating_sub(panel_height);

                // Pruned indicator
                if pruned_count > 0 && y - model.scroll_row < max_y {
                    let pruned_text = format!(" ↑ {} lines hidden ", pruned_count);
                    let pruned_line =
                        Paragraph::new(pruned_text).style(Style::default().fg(colors::MUTED));
                    f.render_widget(
                        pruned_line,
                        Rect::new(
                            2,
                            (y - model.scroll_row) as u16 + HEADER_HEIGHT as u16,
                            size.width.saturating_sub(4),
                            1,
                        ),
                    );
                    y += 1;
                    // Reduce content lines by 1 since pruned indicator takes a row
                    panel_height = panel_height.saturating_sub(1);
                }

                if panel_height > 0 {
                    let output_lines: Vec<String> = task
                        .output
                        .iter()
                        .rev()
                        .take(panel_height)
                        .cloned()
                        .collect();

                    let output_text: Vec<Line> = output_lines
                        .iter()
                        .rev()
                        .map(|line| colorize_output(line))
                        .collect();

                    let output_paragraph = Paragraph::new(output_text)
                        .style(Style::default().fg(colors::TEXT))
                        .block(
                            Block::default()
                                .borders(Borders::LEFT)
                                .border_style(Style::default().fg(colors::MUTED)),
                        );
                    f.render_widget(
                        output_paragraph,
                        Rect::new(
                            2,
                            (y - model.scroll_row) as u16 + HEADER_HEIGHT as u16,
                            size.width.saturating_sub(4),
                            panel_height as u16,
                        ),
                    );
                    y += panel_height;
                }
                y += 1; // spacing after panel
            }
        } else {
            y += task.rendered_height();
        }
    }

    // Footer with counts
    let mut ok_count = 0;
    let mut running_count = 0;
    let mut pending_count = 0;
    let mut error_count = 0;

    for task in &model.tasks {
        match task.status {
            TaskStatus::Success => ok_count += 1,
            TaskStatus::Running => running_count += 1,
            TaskStatus::Pending => pending_count += 1,
            TaskStatus::Error => error_count += 1,
        }
    }

    let mut footer_spans = Vec::new();

    if ok_count > 0 {
        footer_spans.push(Span::styled(
            format!("✓ {} ok ", ok_count),
            Style::default().fg(colors::OK),
        ));
    }
    if running_count > 0 {
        footer_spans.push(Span::styled(
            format!("{} {} running ", SPINNER_FRAMES[spinner_idx], running_count),
            Style::default().fg(colors::RUNNING),
        ));
    }
    if pending_count > 0 {
        footer_spans.push(Span::styled(
            format!("· {} pending ", pending_count),
            Style::default().fg(colors::PENDING),
        ));
    }
    if error_count > 0 {
        footer_spans.push(Span::styled(
            format!("✗ {} error ", error_count),
            Style::default().fg(colors::FAILED),
        ));
    }

    let footer = Paragraph::new(Line::from(footer_spans));
    f.render_widget(
        footer,
        Rect::new(
            1,
            size.height.saturating_sub(1),
            size.width.saturating_sub(2),
            1,
        ),
    );

    // Footer separator
    let footer_sep =
        Paragraph::new("─".repeat(size.width as usize)).style(Style::default().fg(colors::MUTED));
    f.render_widget(
        footer_sep,
        Rect::new(0, size.height.saturating_sub(2), size.width, 1),
    );
}
