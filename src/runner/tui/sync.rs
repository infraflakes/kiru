use super::render;
use super::{Model, SPINNER_FRAMES};
use crate::runner::colors;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

/// Extract the meaningful payload from a sync summary line by stripping
/// the known prefix ("skip  ", "exists  ", "clone  "). Returns the
/// line unchanged if no recognised prefix is found.
fn sync_message(line: &str) -> &str {
    if let Some(rest) = line
        .strip_prefix("skip  ")
        .or_else(|| line.strip_prefix("exists  "))
        .or_else(|| line.strip_prefix("clone  "))
    {
        if let Some(pos) = rest.find(' ') {
            rest[pos + 1..].trim()
        } else {
            rest.trim()
        }
    } else {
        line
    }
}

/// Return the most recent output line of a task, extracted to its
/// meaningful message via `sync_message`.
fn current_display(task: &super::Task) -> String {
    if task.output.is_empty() {
        return String::new();
    }
    task.output
        .last()
        .map(|line| sync_message(line).to_string())
        .unwrap_or_default()
}

/// Render the TUI frame for a `sync` command: a header showing sync
/// progress and a per-project status line for each entry.
pub fn render_sync_output(frame: &mut Frame, model: &Model, spinner_idx: usize) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }

    let mut y_pos = area.y;
    let all_done = model.all_done();
    let (ok_count, err_count) = model.success_and_error_counts();
    let total = model.tasks.len();

    let header = if !all_done {
        format!(
            "{} Syncing projects ({}/{})",
            SPINNER_FRAMES[spinner_idx],
            ok_count + err_count,
            total
        )
    } else if err_count > 0 {
        format!("✗ {} synced, {} failed", ok_count, err_count)
    } else {
        format!("✓ All {} synced", ok_count)
    };
    let header_color = if all_done {
        if err_count > 0 {
            colors::FAILED
        } else {
            colors::OK
        }
    } else {
        colors::RUNNING
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            &header,
            Style::default().fg(header_color),
        ))),
        Rect::new(area.x, y_pos, area.width, 1),
    );
    y_pos += 1;

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
