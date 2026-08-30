use ratatui::style::Color;
use std::io::Write;

/// ANSI escape code to reset all formatting.
pub(crate) const RESET: &str = "\x1b[0m";

/// Green ANSI color for success status.
pub(crate) const OK_ANSI: &str = "\x1b[92m";
/// Red ANSI color for failure status.
pub(crate) const FAILED_ANSI: &str = "\x1b[91m";

/// Bright-yellow ANSI escape for every "active" semantic color so the escape
/// is defined exactly once.
pub(crate) const BRIGHT_YELLOW_ANSI: &str = "\x1b[93m";
/// Gray ANSI escape for muted/pending text.
pub(crate) const GRAY_ANSI: &str = "\x1b[90m";

/// Bold ANSI escape code.
pub(crate) const BOLD: &str = "\x1b[1m";
/// Yellow ANSI color.
pub(crate) const YELLOW: &str = "\x1b[33m";
/// Cyan ANSI color.
pub(crate) const CYAN: &str = "\x1b[36m";
/// Bold cyan ANSI color.
pub(crate) const BOLD_CYAN: &str = "\x1b[1;36m";

/// Bright white text color.
pub(crate) const TEXT_ANSI: &str = "\x1b[97m";
/// Blue color for `exec` statements.
pub(crate) const EXEC_ANSI: &str = "\x1b[94m";
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

/// Prefix written before a `log` statement's text in captured output.
/// Single source of truth: `context.rs` emits it and
/// `colored_line_parts` parses it, so the two must never diverge.
pub(crate) const LOG_PREFIX: &str = "log  ";
/// Prefix written before an `exec` statement's command in captured output.
pub(crate) const EXEC_PREFIX: &str = "exec ";
/// Prefix written before a `cd` statement's target in captured output.
pub(crate) const CD_PREFIX: &str = "cd   ";
/// Prefix written before an `env` statement's keys in captured output.
pub(crate) const ENV_PREFIX: &str = "env  ";

/// Sync progress-line prefixes, shared by the line emitters (sync runner)
/// and the TUI payload stripper (`sync_message`) so the two never diverge.
/// Order: skip, update, clone.
pub(crate) const SYNC_PREFIXES: [&str; 3] = ["skip  ", "update  ", "clone  "];

/// Decompose an output line into (indent, prefix, ANSI color, rest) for
/// rendering.  Used by both terminal and TUI output paths.
pub(crate) fn colored_line_parts(line: &str) -> (usize, &'static str, &'static str, &str) {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    if let Some(rest) = trimmed.strip_prefix(LOG_PREFIX) {
        (indent, LOG_PREFIX, BRIGHT_YELLOW_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix(EXEC_PREFIX) {
        (indent, EXEC_PREFIX, EXEC_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix(CD_PREFIX) {
        (indent, CD_PREFIX, BRIGHT_YELLOW_ANSI, rest)
    } else if let Some(rest) = trimmed.strip_prefix(ENV_PREFIX) {
        (indent, ENV_PREFIX, ENV_ANSI, rest)
    } else {
        (0, "", TEXT_ANSI, line)
    }
}

/// Render a captured output line as a fully colored string (indent left
/// uncolored, then color + prefix + rest + reset). Single implementation
/// shared by every sink: the `Write` path (`write_colored_line`) and the
/// `String` buffer path (`write_colored_line_buf`).
pub(crate) fn colored_line_string(line: &str) -> String {
    let (indent, prefix, color, rest) = colored_line_parts(line);
    format!("{}{}{}{}{}", &line[..indent], color, prefix, rest, RESET)
}

/// Write a single colored output line to a writer.
pub(crate) fn write_colored_line(line: &str, writer: &mut impl Write) {
    let _ = writer.write_all(colored_line_string(line).as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every emitted prefix must be re-recognized by the parser, otherwise a
    /// change to one side silently breaks color detection.
    #[test]
    fn colored_line_parts_recognizes_every_prefix() {
        let cases = [
            (LOG_PREFIX, BRIGHT_YELLOW_ANSI),
            (EXEC_PREFIX, EXEC_ANSI),
            (CD_PREFIX, BRIGHT_YELLOW_ANSI),
            (ENV_PREFIX, ENV_ANSI),
        ];
        for (prefix, expected_color) in cases {
            let line = format!("  {}{}", prefix, "payload");
            let (indent, parsed_prefix, color, rest) = colored_line_parts(&line);
            assert_eq!(indent, 2, "prefix {prefix:?} kept its indent");
            assert_eq!(parsed_prefix, prefix, "prefix {prefix:?} round-tripped");
            assert_eq!(color, expected_color);
            assert_eq!(rest, "payload");
        }
    }

    #[test]
    fn colored_line_string_round_trips() {
        let line = format!("{}{}", LOG_PREFIX, "hello");
        let rendered = colored_line_string(&line);
        assert!(rendered.starts_with(BRIGHT_YELLOW_ANSI));
        assert!(rendered.contains(LOG_PREFIX));
        assert!(rendered.contains("hello"));
        assert!(rendered.ends_with(RESET));
    }
}
