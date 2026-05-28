use super::{MAX_PANEL_HEIGHT, Model, SPINNER_FRAMES, Task, TaskStatus};
use crate::colors;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::io::{self, Write};

fn task_marker(task: &Task, spinner_idx: usize) -> String {
    if task.finalized {
        if task.status == TaskStatus::Success {
            "✓".to_string()
        } else {
            "✗".to_string()
        }
    } else {
        match task.status {
            TaskStatus::Pending => "·".to_string(),
            TaskStatus::Running => SPINNER_FRAMES[spinner_idx].to_string(),
            TaskStatus::Success | TaskStatus::Error => unreachable!(),
        }
    }
}

fn render_summary(f: &mut Frame, yo: u16, model: &Model, spinner_idx: usize, width: u16) {
    let mut ok_count = 0;
    let mut running_count = 0;
    let mut pending_count = 0;
    let mut error_count = 0;

    for task in &model.tasks {
        match task.status {
            TaskStatus::Success => ok_count += 1,
            TaskStatus::Running => running_count += 1,
            TaskStatus::Pending => pending_count += 1,
            TaskStatus::Error => error_count += 1,
        }
    }

    let mut spans: Vec<Span> = Vec::with_capacity(4);

    if ok_count > 0 {
        spans.push(Span::styled(
            format!("✓ {} ok ", ok_count),
            Style::default().fg(colors::OK),
        ));
    }
    if running_count > 0 {
        spans.push(Span::styled(
            format!("{} {} running ", SPINNER_FRAMES[spinner_idx], running_count),
            Style::default().fg(colors::RUNNING),
        ));
    }
    if pending_count > 0 {
        spans.push(Span::styled(
            format!("· {} pending ", pending_count),
            Style::default().fg(colors::PENDING),
        ));
    }
    if error_count > 0 {
        spans.push(Span::styled(
            format!("✗ {} failed ", error_count),
            Style::default().fg(colors::FAILED),
        ));
    }

    let summary = Paragraph::new(Line::from(spans));
    f.render_widget(summary, Rect::new(0, yo, width, 1));
}

pub fn render(f: &mut Frame, model: &Model, spinner_idx: usize) {
    let area = f.area();
    f.render_widget(Clear, area);
    if area.height < 1 {
        return;
    }
    render_summary(f, area.y, model, spinner_idx, area.width);
}

pub fn write_colored_line(line: &str, w: &mut impl Write) -> io::Result<()> {
    let trimmed = line.trim_start();

    if !trimmed.contains(' ') && trimmed.contains('(') && trimmed.ends_with(')') {
        write!(w, "{}{}{}", colors::EXEC_ANSI, line, colors::RESET)?;
        return Ok(());
    }

    let (prefix, ansi_color) = if trimmed.starts_with("log  ") {
        ("log  ", colors::LOG_ANSI)
    } else if trimmed.starts_with("exec ") {
        ("exec ", colors::EXEC_ANSI)
    } else if trimmed.starts_with("cd   ") {
        ("cd   ", colors::CD_ANSI)
    } else if trimmed.starts_with("env  ") {
        ("env  ", colors::ENV_ANSI)
    } else {
        write!(w, "{}{line}{}", colors::TEXT_ANSI, colors::RESET)?;
        return Ok(());
    };

    let indent = line.len() - trimmed.len();

    if indent > 0 {
        write!(w, "{}", &line[..indent])?;
    }
    write!(
        w,
        "{ansi_color}{prefix}{content}{reset}",
        ansi_color = ansi_color,
        prefix = prefix,
        content = &trimmed[prefix.len()..],
        reset = colors::RESET
    )
}

pub fn dump_final(model: &Model, w: &mut impl Write) -> io::Result<()> {
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

    writeln!(w)?;
    writeln!(w)?;

    for task in &model.tasks {
        let color = match task.status {
            TaskStatus::Success => colors::OK_ANSI,
            TaskStatus::Running => colors::RUNNING_ANSI,
            TaskStatus::Pending => colors::PENDING_ANSI,
            TaskStatus::Error => colors::FAILED_ANSI,
        };
        let marker = task_marker(task, 0);

        writeln!(w, " {}{}{} {}", color, marker, colors::RESET, task.name)?;

        if !task.output.is_empty() {
            let total = task.output.len();
            let panel = total.min(MAX_PANEL_HEIGHT);
            let pruned = total - panel;

            if pruned > 0 {
                writeln!(
                    w,
                    "   {}↑ {} lines hidden {}",
                    colors::MUTED_ANSI,
                    pruned,
                    colors::RESET
                )?;
            }

            for line in task.output.iter().rev().take(panel).rev() {
                write!(w, "  {}  {}", colors::MUTED_ANSI, colors::RESET)?;
                write_colored_line(line, w)?;
                writeln!(w)?;
            }
        }
        writeln!(w)?;
    }

    if err_count > 0 {
        writeln!(w, "{} done, {} failed", ok_count, err_count)?;
    } else {
        writeln!(w, "✓ all {} passed", ok_count)?;
    }

    Ok(())
}
