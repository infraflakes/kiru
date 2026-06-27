use super::render;
use super::{Model, SPINNER_FRAMES, TaskStatus};
use crate::runner::colors;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

/// Extract just the meaningful message from a sync summary line.
/// For git output lines (no known prefix), returns the line as-is.
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

fn current_display(task: &super::Task) -> String {
    if task.output.is_empty() {
        return String::new();
    }
    if task.finalized {
        // Show summary (first line) for finalized tasks
        sync_message(&task.output[0]).to_string()
    } else {
        // Show live output (last line) for running/pending tasks
        sync_message(task.output.last().unwrap()).to_string()
    }
}

pub fn render(f: &mut Frame, model: &Model, spinner_idx: usize) {
    let area = f.area();
    f.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }

    let mut y = area.y;
    let all_done = model.all_done();
    let done_count = model
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::Success | TaskStatus::Error))
        .count();
    let total = model.tasks.len();

    let header = if all_done {
        "✓ All projects synced".to_string()
    } else {
        format!(
            "{} Syncing projects ({}/{})",
            SPINNER_FRAMES[spinner_idx], done_count, total
        )
    };
    let header_color = if all_done {
        colors::OK
    } else {
        colors::RUNNING
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            &header,
            Style::default().fg(header_color),
        ))),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 1;

    for (ci, chain) in model.chains.iter().enumerate() {
        if y >= area.y + area.height {
            break;
        }

        let conn = if ci == model.chains.len() - 1 {
            "└──"
        } else {
            "├──"
        };

        if let Some(task) = model.tasks.get(chain.task_start) {
            let color = render::status_color(task.status);
            let display = current_display(task);
            let line = format!("{} [{}]  {}", conn, task.name, display);
            let span = Span::styled(line, Style::default().fg(color));
            f.render_widget(
                Paragraph::new(Line::from(span)),
                Rect::new(area.x, y, area.width, 1),
            );
        }
        y += 1;
    }
}

pub fn format_final_output(_model: &Model) -> String {
    String::new()
}
