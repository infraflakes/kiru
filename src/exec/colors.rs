use ratatui::style::Color;

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
/// Green ratatui color for success status.
pub(crate) const OK: Color = Color::Indexed(10);
/// Yellow ratatui color for running status.
pub(crate) const RUNNING: Color = Color::Indexed(11);
/// Red ratatui color for failure status.
pub(crate) const FAILED: Color = Color::Indexed(9);
/// Gray ratatui color for pending status.
pub(crate) const PENDING: Color = Color::Indexed(8);
/// White ratatui color for cancelled status: distinct from red so the eye
/// goes to the task that actually failed, not to fail-fast victims.
pub(crate) const CANCELLED: Color = Color::White;

/// White ANSI color for cancelled status in text output.
pub(crate) const CANCELLED_ANSI: &str = "\x1b[97m";

/// Sync progress-line prefixes, shared by the line emitters (sync runner)
/// and the TUI payload stripper (`sync_message`) so the two never diverge.
pub(crate) const SYNC_UPDATE_PREFIX: &str = "update  ";
pub(crate) const SYNC_CLONE_PREFIX: &str = "clone  ";

/// Render a command-output line: ANSI already present passes through,
/// everything else becomes plain white text. Indentation and structure
/// live on the task lines, not in the captured payload.
pub(crate) fn colored_line_string(line: &str) -> String {
    if line.contains("\x1b[") {
        return line.to_string();
    }
    format!("{}{}{}", TEXT_ANSI, line, RESET)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colored_line_string_round_trips() {
        let rendered = colored_line_string("hello");
        assert_eq!(rendered, format!("{TEXT_ANSI}hello{RESET}"));
        let already_colored = format!("{OK_ANSI}done{RESET}");
        assert_eq!(colored_line_string(&already_colored), already_colored);
    }
}
