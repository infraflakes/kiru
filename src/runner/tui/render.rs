use super::{SPINNER_FRAMES, Task, TaskStatus};
use crate::runner::colors;
use ratatui::style::Color;

pub const SEPARATOR_WIDTH: usize = 78;

pub fn task_marker(task: &Task, spinner_idx: usize) -> String {
    if task.finalized {
        if task.status == TaskStatus::Success {
            "✓".to_string()
        } else {
            "✗".to_string()
        }
    } else {
        match task.status {
            TaskStatus::Success => "✓".to_string(),
            TaskStatus::Error => "✗".to_string(),
            TaskStatus::Pending => "·".to_string(),
            TaskStatus::Running => SPINNER_FRAMES[spinner_idx].to_string(),
        }
    }
}

pub fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Success => "ok",
        TaskStatus::Running => "running",
        TaskStatus::Pending => "pending",
        TaskStatus::Error => "failed",
    }
}

pub fn status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Success => colors::OK,
        TaskStatus::Running => colors::RUNNING,
        TaskStatus::Pending => colors::PENDING,
        TaskStatus::Error => colors::FAILED,
    }
}

pub fn write_colored_line_buf(buf: &mut String, line: &str) {
    let (indent, prefix, color, rest) = colors::colored_line_parts(line);
    if indent > 0 {
        buf.push_str(&line[..indent]);
    }
    buf.push_str(color);
    buf.push_str(prefix);
    buf.push_str(rest);
    buf.push_str(colors::RESET);
}

pub fn write_separator(buf: &mut String, label: &str) {
    let sep_len = SEPARATOR_WIDTH.saturating_sub(label.len() + 4);
    let left = sep_len / 2;
    let right = sep_len - left;
    buf.push_str(colors::MUTED_ANSI);
    for _ in 0..left {
        buf.push('─');
    }
    buf.push(' ');
    buf.push_str(label);
    buf.push(' ');
    for _ in 0..right {
        buf.push('─');
    }
    buf.push_str(colors::RESET);
    buf.push('\n');
}
