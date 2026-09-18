use super::SPINNER_FRAMES;
use super::model::TaskStatus;
use crate::exec::colors;
use ratatui::style::Color;

/// Return a single-character visual marker for a task status: spinning
/// frames for running, checkmark/cross for success/error, a filled square
/// for cancelled, a middle dot for pending.
pub(crate) fn status_glyph(status: TaskStatus, spinner_idx: usize) -> String {
    match status {
        TaskStatus::Success => "✓".to_string(),
        TaskStatus::Error => "✗".to_string(),
        TaskStatus::Cancelled => "■".to_string(),
        TaskStatus::Skipped => "-".to_string(),
        TaskStatus::Pending => "·".to_string(),
        TaskStatus::Running => SPINNER_FRAMES[spinner_idx].to_string(),
    }
}

/// Return the ratatui `Color` associated with a task status (green for
/// success, yellow for running, gray for pending, red for error, white for
/// cancelled).
pub(crate) fn status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Success => colors::OK,
        TaskStatus::Running => colors::RUNNING,
        TaskStatus::Pending => colors::PENDING,
        TaskStatus::Error => colors::FAILED,
        TaskStatus::Cancelled => colors::CANCELLED,
        TaskStatus::Skipped => colors::PENDING,
    }
}
