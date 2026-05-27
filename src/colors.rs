use ratatui::style::Color;

pub const RESET: &str = "\x1b[0m";

pub const OK_ANSI: &str = "\x1b[92m";
pub const RUNNING_ANSI: &str = "\x1b[93m";
pub const FAILED_ANSI: &str = "\x1b[91m";
pub const PENDING_ANSI: &str = "\x1b[90m";

pub const MUTED_ANSI: &str = "\x1b[90m";
pub const TEXT_ANSI: &str = "\x1b[97m";

pub const LOG_ANSI: &str = "\x1b[93m";
pub const EXEC_ANSI: &str = "\x1b[94m";
pub const CD_ANSI: &str = "\x1b[93m";
pub const ENV_ANSI: &str = "\x1b[95m";

pub const OK: Color = Color::Indexed(10);
pub const RUNNING: Color = Color::Indexed(11);
pub const FAILED: Color = Color::Indexed(9);
pub const PENDING: Color = Color::Indexed(8);
