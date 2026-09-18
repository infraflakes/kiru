use crate::exec::colors;
use crate::exec::model::{Display, NodeState, SPINNER_FRAMES, TaskStatus};
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
    let rest = if let Some(rest) = line.strip_prefix(colors::SYNC_UPDATE_PREFIX) {
        rest
    } else if let Some(rest) = line.strip_prefix(colors::SYNC_CLONE_PREFIX) {
        rest
    } else {
        return line;
    };
    match rest.find(' ') {
        Some(pos) => rest[pos + 1..].trim(),
        None => rest.trim(),
    }
}

/// Return the most recent output line of a project, extracted to its
/// meaningful message via `sync_message`.
fn current_display(state: &NodeState) -> String {
    state
        .output
        .last()
        .map(|line| sync_message(line).to_string())
        .unwrap_or_default()
}

/// Render the TUI frame for a `sync` command: a header showing sync
/// progress and a per-project status line for each entry.
pub(crate) fn render_sync_output(frame: &mut Frame, display: &Display, spinner_idx: usize) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }

    let mut y_pos = area.y;

    // Live progress only: the per-project glyphs below are the report, so
    // the header disappears once everything is done instead of announcing
    // final counts.
    if !display.all_done() {
        let total = display.len();
        let done = display
            .states()
            .iter()
            .filter(|state| {
                matches!(
                    state.status,
                    TaskStatus::Success
                        | TaskStatus::Error
                        | TaskStatus::Cancelled
                        | TaskStatus::Skipped
                )
            })
            .count();
        let header = format!(
            "{} Syncing projects ({}/{})",
            SPINNER_FRAMES[spinner_idx % SPINNER_FRAMES.len()],
            done,
            total
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

    let count = display.len();
    for (index, state) in display.states().iter().enumerate() {
        if y_pos >= area.y + area.height {
            break;
        }

        let connector = if index + 1 == count {
            "└──"
        } else {
            "├──"
        };
        let label = state.label.as_deref().unwrap_or_default();
        let line = format!("{} [{}]  {}", connector, label, current_display(state));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                line,
                Style::default().fg(state.status.color()),
            ))),
            Rect::new(area.x, y_pos, area.width, 1),
        );
        y_pos += 1;
    }
}
