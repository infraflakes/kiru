use ratatui::style::Color;
use std::io::Write;

pub(crate) const RESET: &str = "\x1b[0m";

pub(crate) const OK_ANSI: &str = "\x1b[92m";
pub(crate) const RUNNING_ANSI: &str = "\x1b[93m";
pub(crate) const FAILED_ANSI: &str = "\x1b[91m";
pub(crate) const PENDING_ANSI: &str = "\x1b[90m";

pub(crate) const MUTED_ANSI: &str = "\x1b[90m";
pub(crate) const TEXT_ANSI: &str = "\x1b[97m";

pub(crate) const LOG_ANSI: &str = "\x1b[93m";
pub(crate) const EXEC_ANSI: &str = "\x1b[94m";
pub(crate) const CD_ANSI: &str = "\x1b[93m";
pub(crate) const ENV_ANSI: &str = "\x1b[95m";

pub(crate) const OK: Color = Color::Indexed(10);
pub(crate) const RUNNING: Color = Color::Indexed(11);
pub(crate) const FAILED: Color = Color::Indexed(9);
pub(crate) const PENDING: Color = Color::Indexed(8);

pub(crate) fn colored_line_parts(line: &str) -> (usize, &'static str, &'static str, &str) {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    if !trimmed.contains(' ') && trimmed.contains('(') && trimmed.ends_with(')') {
        (0, "", EXEC_ANSI, line)
    } else if let Some(rest) = trimmed.strip_prefix("log  ") {
        (indent, "log  ", LOG_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix("exec ") {
        (indent, "exec ", EXEC_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix("cd   ") {
        (indent, "cd   ", CD_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix("env  ") {
        (indent, "env  ", ENV_ANSI, rest)
    } else {
        (0, "", TEXT_ANSI, line)
    }
}

pub(crate) fn write_colored_line(line: &str, w: &mut impl Write) {
    let (indent, prefix, color, rest) = colored_line_parts(line);
    if indent > 0 {
        let _ = write!(w, "{}", &line[..indent]);
    }
    let _ = write!(w, "{}{}{}{}", color, prefix, rest, RESET);
}
