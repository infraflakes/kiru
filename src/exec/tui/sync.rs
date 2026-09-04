use super::SPINNER_FRAMES;
use super::model::{Model, TaskStatus};
use super::render;
use crate::exec::colors;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

/// Extract the meaningful payload from a sync summary line by stripping the
/// known sync prefix. Returns the line unchanged if no prefix matches.
fn sync_message(line: &str) -> &str {
    let rest = if let Some(rest) = line.strip_prefix(crate::exec::colors::SYNC_UPDATE_PREFIX) {
        rest
    } else if let Some(rest) = line.strip_prefix(crate::exec::colors::SYNC_CLONE_PREFIX) {
        rest
    } else {
        return line;
    };
    match rest.find(' ') {
        Some(pos) => rest[pos + 1..].trim(),
        None => rest.trim(),
    }
}

/// Return the most recent output line of a task, extracted to its
/// meaningful message via `sync_message`.
fn current_display(task: &super::TaskRow) -> String {
    task.output
        .last()
        .map(|line| sync_message(line).to_string())
        .unwrap_or_default()
}

/// Render the TUI frame for a `sync` command: a header showing sync
/// progress and a per-project status line for each entry.
pub(crate) fn render_sync_output(frame: &mut Frame, model: &Model, spinner_idx: usize) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }

    let mut y_pos = area.y;

    // Live progress only: the per-project glyphs below are the report, so
    // the header disappears once everything is done instead of announcing
    // final counts.
    if !model.all_done() {
        let total = model.tasks.len();
        let done = model
            .tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    TaskStatus::Success | TaskStatus::Error | TaskStatus::Cancelled
                )
            })
            .count();
        let header = format!(
            "{} Syncing projects ({}/{})",
            SPINNER_FRAMES[spinner_idx], done, total
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                &header,
                Style::default().fg(colors::RUNNING),
            ))),
            Rect::new(area.x, y_pos, area.width, 1),
        );
        y_pos += 1;
    }

    for (chain_idx, chain) in model.chains.iter().enumerate() {
        if y_pos >= area.y + area.height {
            break;
        }

        let conn = if chain_idx == model.chains.len() - 1 {
            "└──"
        } else {
            "├──"
        };

        if let Some(task) = model.tasks.get(chain.task_start) {
            let color = render::status_color(task.status);
            let display = current_display(task);
            let line = format!("{} [{}]  {}", conn, task.name, display);
            let span = Span::styled(line, Style::default().fg(color));
            frame.render_widget(
                Paragraph::new(Line::from(span)),
                Rect::new(area.x, y_pos, area.width, 1),
            );
        }
        y_pos += 1;
    }
}
