use ratatui::style::Color;
use std::io::Write;

/// ANSI escape code to reset all formatting.
pub(crate) const RESET: &str = "\x1b[0m";

/// Green ANSI color for success status.
pub(crate) const OK_ANSI: &str = "\x1b[92m";
/// Yellow ANSI color for running status.
pub(crate) const RUNNING_ANSI: &str = "\x1b[93m";
/// Red ANSI color for failure status.
pub(crate) const FAILED_ANSI: &str = "\x1b[91m";
/// Gray ANSI color for pending status.
pub(crate) const PENDING_ANSI: &str = "\x1b[90m";

/// Muted text color.
pub(crate) const MUTED_ANSI: &str = "\x1b[90m";
/// Bright white text color.
pub(crate) const TEXT_ANSI: &str = "\x1b[97m";

/// Yellow color for `log` statements.
pub(crate) const LOG_ANSI: &str = "\x1b[93m";
/// Blue color for `exec` statements.
pub(crate) const EXEC_ANSI: &str = "\x1b[94m";
/// Yellow color for `cd` statements.
pub(crate) const CD_ANSI: &str = "\x1b[93m";
/// Magenta color for `env` statements.
pub(crate) const ENV_ANSI: &str = "\x1b[95m";

/// Green ratatui color for success status.
pub(crate) const OK: Color = Color::Indexed(10);
/// Yellow ratatui color for running status.
pub(crate) const RUNNING: Color = Color::Indexed(11);
/// Red ratatui color for failure status.
pub(crate) const FAILED: Color = Color::Indexed(9);
/// Gray ratatui color for pending status.
pub(crate) const PENDING: Color = Color::Indexed(8);

/// Decompose an output line into (indent, prefix, ANSI color, rest) for
/// rendering.  Used by both terminal and TUI output paths.
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

/// Write a single colored output line to a writer.
pub(crate) fn write_colored_line(line: &str, writer: &mut impl Write) {
    let (indent, prefix, color, rest) = colored_line_parts(line);
    if indent > 0 {
        let _ = write!(writer, "{}", &line[..indent]);
    }
    let _ = write!(writer, "{}{}{}{}", color, prefix, rest, RESET);
}
