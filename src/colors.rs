use ratatui::style::Color;

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
