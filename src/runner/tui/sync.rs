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
    } else if let Some(last) = task.output.last() {
        sync_message(last).to_string()
    } else {
        String::new()
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
    let ok_count = model
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Success)
        .count();
    let err_count = model
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Error)
        .count();
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
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            &header,
            Style::default().fg(header_color),
        ))),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 1;

    for (chain_idx, chain) in model.chains.iter().enumerate() {
        if y >= area.y + area.height {
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
            f.render_widget(
                Paragraph::new(Line::from(span)),
                Rect::new(area.x, y, area.width, 1),
            );
        }
        y += 1;
    }
}
