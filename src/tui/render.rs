use super::{MAX_PANEL_HEIGHT, Model, SPINNER_FRAMES, Task, TaskStatus};
use crate::colors;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::io::Write;

const SEPARATOR_WIDTH: usize = 78;

fn task_marker(task: &Task, spinner_idx: usize) -> String {
    if task.finalized {
        if task.status == TaskStatus::Success {
            "✓".to_string()
        } else if task.status == TaskStatus::Skipped {
            "−".to_string()
        } else {
            "✗".to_string()
        }
    } else {
        match task.status {
            TaskStatus::Pending => "·".to_string(),
            TaskStatus::Running => SPINNER_FRAMES[spinner_idx].to_string(),
            TaskStatus::Success | TaskStatus::Error | TaskStatus::Skipped => unreachable!(),
        }
    }
}

fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Success => "ok",
        TaskStatus::Running => "running",
        TaskStatus::Pending => "pending",
        TaskStatus::Error => "failed",
        TaskStatus::Skipped => "skipped",
    }
}

fn status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Success => colors::OK,
        TaskStatus::Running => colors::RUNNING,
        TaskStatus::Pending => colors::PENDING,
        TaskStatus::Error => colors::FAILED,
        TaskStatus::Skipped => colors::PENDING,
    }
}

pub fn render(f: &mut Frame, model: &Model, spinner_idx: usize) {
    let area = f.area();
    f.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }

    let mut y = area.y;
    for chain in &model.chains {
        if y >= area.y + area.height {
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
        f.render_widget(
            Paragraph::new(Line::from(header_span)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;

        for ti in 0..chain.task_count {
            if y >= area.y + area.height {
                break;
            }
            if let Some(task) = model.tasks.get(chain.task_start + ti) {
                let tmarker = task_marker(task, spinner_idx);
                let tcolor = status_color(task.status);
                let line = format!(
                    "│   {}  {} {}",
                    task.name,
                    tmarker,
                    status_label(task.status),
                );
                let span = Span::styled(line, Style::default().fg(tcolor));
                f.render_widget(
                    Paragraph::new(Line::from(span)),
                    Rect::new(area.x, y, area.width, 1),
                );
            }
            y += 1;
        }
    }
}

fn colored_line_parts(line: &str) -> (usize, &'static str, &'static str, &str) {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    if !trimmed.contains(' ') && trimmed.contains('(') && trimmed.ends_with(')') {
        (0, "", colors::EXEC_ANSI, line)
    } else if let Some(rest) = trimmed.strip_prefix("log  ") {
        (indent, "log  ", colors::LOG_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix("exec ") {
        (indent, "exec ", colors::EXEC_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix("cd   ") {
        (indent, "cd   ", colors::CD_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix("env  ") {
        (indent, "env  ", colors::ENV_ANSI, rest)
    } else {
        (0, "", colors::TEXT_ANSI, line)
    }
}

pub fn write_colored_line(line: &str, w: &mut impl Write) {
    let (indent, prefix, color, rest) = colored_line_parts(line);
    if indent > 0 {
        let _ = write!(w, "{}", &line[..indent]);
    }
    let _ = write!(w, "{}{}{}{}", color, prefix, rest, colors::RESET);
}

fn write_colored_line_buf(buf: &mut String, line: &str) {
    let (indent, prefix, color, rest) = colored_line_parts(line);
    if indent > 0 {
        buf.push_str(&line[..indent]);
    }
    buf.push_str(color);
    buf.push_str(prefix);
    buf.push_str(rest);
    buf.push_str(colors::RESET);
}

fn write_separator(buf: &mut String, label: &str) {
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

pub fn dump_final(model: &Model) -> String {
    let mut buf = String::new();
    buf.push('\n');

    for chain in &model.chains {
        write_separator(&mut buf, &chain.label);

        for ti in 0..chain.task_count {
            if let Some(task) = model.tasks.get(chain.task_start + ti) {
                let color = match task.status {
                    TaskStatus::Success => colors::OK_ANSI,
                    TaskStatus::Running => colors::RUNNING_ANSI,
                    TaskStatus::Pending => colors::PENDING_ANSI,
                    TaskStatus::Error => colors::FAILED_ANSI,
                    TaskStatus::Skipped => colors::MUTED_ANSI,
                };
                let marker = task_marker(task, 0);

                buf.push(' ');
                buf.push_str(color);
                buf.push_str(&marker);
                buf.push_str(colors::RESET);
                buf.push(' ');
                buf.push_str(&task.name);
                buf.push('\n');

                if !task.output.is_empty() {
                    let total = task.output.len();
                    let panel = total.min(MAX_PANEL_HEIGHT);
                    let pruned = total - panel;

                    if pruned > 0 {
                        buf.push_str("   ");
                        buf.push_str(colors::MUTED_ANSI);
                        buf.push('↑');
                        buf.push(' ');
                        buf.push_str(&pruned.to_string());
                        buf.push_str(" lines hidden ");
                        buf.push_str(colors::RESET);
                        buf.push('\n');
                    }

                    for line in task.output.iter().rev().take(panel).rev() {
                        buf.push_str("  ");
                        buf.push_str(colors::MUTED_ANSI);
                        buf.push_str("  ");
                        buf.push_str(colors::RESET);
                        write_colored_line_buf(&mut buf, line);
                        buf.push('\n');
                    }
                }
                buf.push('\n');
            }
        }
    }

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
    let skipped_count = model
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Skipped)
        .count();

    if err_count > 0 {
        if skipped_count > 0 {
            buf.push_str(&format!(
                "{} done, {} failed, {} skipped\n",
                ok_count, err_count, skipped_count
            ));
        } else {
            buf.push_str(&format!("{} done, {} failed\n", ok_count, err_count));
        }
    } else if skipped_count > 0 {
        buf.push_str(&format!("✓ all passed, {} skipped\n", skipped_count));
    } else {
        buf.push_str(&format!("✓ all {} passed\n", ok_count));
    }

    buf
}
