use super::{SPINNER_FRAMES, Task, TaskStatus};
use crate::runner::colors;
use ratatui::style::Color;

/// Width in characters of the separator line drawn between chain sections
/// in the final text output.
pub const SEPARATOR_WIDTH: usize = 78;

/// Return a single-character visual marker for a task's state.
/// Spinning frames are used for running tasks; checkmark/cross for
/// finalized tasks; a middle dot for pending tasks.
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

/// Return a short human-readable label for a task status
/// (e.g. "ok", "running", "pending", "failed").
pub fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Success => "ok",
        TaskStatus::Running => "running",
        TaskStatus::Pending => "pending",
        TaskStatus::Error => "failed",
    }
}

/// Return the ratatui `Color` associated with a task status
/// (green for success, yellow for running, gray for pending, red for error).
pub fn status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Success => colors::OK,
        TaskStatus::Running => colors::RUNNING,
        TaskStatus::Pending => colors::PENDING,
        TaskStatus::Error => colors::FAILED,
    }
}

/// Append a single output line to `buf` with ANSI color codes derived from
/// the line prefix (log, exec, cd, env, etc.).
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

/// Write a horizontal separator line with a centered label into `buf`,
/// using 78-character width defined by `SEPARATOR_WIDTH`.
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
