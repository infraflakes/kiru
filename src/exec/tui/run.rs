use super::MAX_PANEL_HEIGHT;
use super::model::{Model, TaskRow, TaskStatus};
use super::render::{status_color, status_glyph};
use crate::exec::colors;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

/// Render the TUI frame for a `run` command: every plan line with its tree
/// prefix, status glyph, label, and project annotation.
pub(crate) fn render_run_output(frame: &mut Frame, model: &Model, spinner_idx: usize) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }

    let bottom = area.y + area.height;
    for (y_pos, task) in (area.y..).zip(model.tasks.iter()) {
        if y_pos >= bottom {
            break;
        }
        let tcolor = status_color(task.status);
        let line = format!(
            "{}{} {}{}",
            task.prefix,
            status_glyph(task.status, spinner_idx),
            task.name,
            task.project
                .as_deref()
                .map(|project| format!(" [{project}]"))
                .unwrap_or_default()
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(line, Style::default().fg(tcolor)))),
            Rect::new(area.x, y_pos, area.width, 1),
        );
    }
}

/// Append one line's final output (status marker, label, and output lines)
/// to the buffer, prefixed so the tree structure stays visible.
fn format_task_output(buf: &mut String, task: &TaskRow) {
    let color = match task.status {
        TaskStatus::Success => colors::OK_ANSI,
        TaskStatus::Running => colors::BRIGHT_YELLOW_ANSI,
        TaskStatus::Pending => colors::GRAY_ANSI,
        TaskStatus::Error => colors::FAILED_ANSI,
        TaskStatus::Cancelled => colors::CANCELLED_ANSI,
        TaskStatus::Skipped => colors::GRAY_ANSI,
    };
    let marker = status_glyph(task.status, 0);
    let project = task
        .project
        .as_deref()
        .map(|project| format!(" [{project}]"))
        .unwrap_or_default();

    buf.push_str(&task.prefix);
    buf.push_str(color);
    buf.push_str(&marker);
    buf.push_str(colors::RESET);
    buf.push(' ');
    buf.push_str(&task.name);
    buf.push_str(&project);
    buf.push('\n');

    if !task.output.is_empty() {
        let total = task.output.len();
        let visible_lines = total.min(MAX_PANEL_HEIGHT);
        let hidden_lines = total - visible_lines;

        if hidden_lines > 0 {
            buf.push_str(&task.output_prefix);
            buf.push_str(colors::GRAY_ANSI);
            buf.push('↑');
            buf.push(' ');
            buf.push_str(&hidden_lines.to_string());
            buf.push_str(" lines hidden ");
            buf.push_str(colors::RESET);
            buf.push('\n');
        }

        for output_line in task.output.iter().rev().take(visible_lines).rev() {
            buf.push_str(&task.output_prefix);
            buf.push_str(&colors::colored_line_string(output_line));
            buf.push('\n');
        }
    }
}

/// Build the final ANSI-colored text dump after all tasks complete: the
/// whole structure (every line, including `async`/`env`/switch arms) with
/// statuses and each line's output hanging underneath. The branches are
/// what make concurrency and nesting visible.
pub(crate) fn format_final_output(model: &Model) -> String {
    let mut buf = String::new();
    buf.push('\n');

    for task in &model.tasks {
        format_task_output(&mut buf, task);
    }

    buf
}
