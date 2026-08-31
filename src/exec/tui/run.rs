use super::render::{self, status_color, status_glyph, status_label};
use super::{MAX_PANEL_HEIGHT, Model, TaskRow, TaskStatus};
use crate::exec::colors;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

/// Render the TUI frame for a `run` command: draw each chain header and
/// its tasks with status markers and spinner animation.
pub(crate) fn render_run_output(frame: &mut Frame, model: &Model, spinner_idx: usize) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }

    let mut y_pos = area.y;
    for chain in &model.chains {
        if y_pos >= area.y + area.height {
            break;
        }

        let ch_status = model.chain_status(chain);
        let ch_color = status_color(ch_status);
        let header_char = match ch_status {
            TaskStatus::Success => "✓",
            TaskStatus::Error => "✗",
            _ => "├─",
        };

        let header = format!("{} {}", header_char, chain.label);
        let header_span = Span::styled(header, Style::default().fg(ch_color));
        frame.render_widget(
            Paragraph::new(Line::from(header_span)),
            Rect::new(area.x, y_pos, area.width, 1),
        );
        y_pos += 1;

        for task_offset in 0..chain.task_count {
            if y_pos >= area.y + area.height {
                break;
            }
            if let Some(task) = model.tasks.get(chain.task_start + task_offset) {
                let tmarker = status_glyph(task.status, spinner_idx);
                let tcolor = status_color(task.status);
                let line = format!(
                    "│   {}  {} {}",
                    task.name,
                    tmarker,
                    status_label(task.status),
                );
                let span = Span::styled(line, Style::default().fg(tcolor));
                frame.render_widget(
                    Paragraph::new(Line::from(span)),
                    Rect::new(area.x, y_pos, area.width, 1),
                );
            }
            y_pos += 1;
        }
    }
}

/// Append a single task's final output (status marker, name, and output lines)
/// to the buffer.
fn format_task_output(buf: &mut String, task: &TaskRow) {
    let color = match task.status {
        TaskStatus::Success => colors::OK_ANSI,
        TaskStatus::Running => colors::BRIGHT_YELLOW_ANSI,
        TaskStatus::Pending => colors::GRAY_ANSI,
        TaskStatus::Error => colors::FAILED_ANSI,
    };
    let marker = status_glyph(task.status, 0);

    buf.push(' ');
    buf.push_str(color);
    buf.push_str(&marker);
    buf.push_str(colors::RESET);
    buf.push(' ');
    buf.push_str(&task.name);
    buf.push('\n');

    if !task.output.is_empty() {
        let total = task.output.len();
        let visible_lines = total.min(MAX_PANEL_HEIGHT);
        let hidden_lines = total - visible_lines;

        if hidden_lines > 0 {
            buf.push_str("   ");
            buf.push_str(colors::GRAY_ANSI);
            buf.push('↑');
            buf.push(' ');
            buf.push_str(&hidden_lines.to_string());
            buf.push_str(" lines hidden ");
            buf.push_str(colors::RESET);
            buf.push('\n');
        }

        for output_line in task.output.iter().rev().take(visible_lines).rev() {
            buf.push_str("  ");
            buf.push_str(colors::GRAY_ANSI);
            buf.push_str("  ");
            buf.push_str(colors::RESET);
            render::write_colored_line_buf(buf, output_line);
            buf.push('\n');
        }
    }
    buf.push('\n');
}

/// Append the run summary (pass/fail counts) to the buffer.
fn format_summary(buf: &mut String, model: &Model) {
    let (ok_count, err_count) = model.success_and_error_counts();
    if err_count > 0 {
        buf.push_str(&format!("{} done, {} failed\n", ok_count, err_count));
    } else {
        buf.push_str(&format!("✓ all {} passed\n", ok_count));
    }
}

/// Build the final ANSI-colored text dump after all tasks complete.
/// Includes per-task status, output lines (truncated to `MAX_PANEL_HEIGHT`),
/// and a summary line with pass/fail counts.
pub(crate) fn format_final_output(model: &Model) -> String {
    let mut buf = String::new();
    buf.push('\n');

    for chain in &model.chains {
        render::write_separator(&mut buf, &chain.label);

        for task_offset in 0..chain.task_count {
            if let Some(task) = model.tasks.get(chain.task_start + task_offset) {
                format_task_output(&mut buf, task);
            }
        }
    }

    format_summary(&mut buf, model);
    buf
}
