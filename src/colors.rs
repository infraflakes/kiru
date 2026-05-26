// Palette matching the Go version (old-go/internal/tui/colors.go)
use ratatui::style::Color;

pub const RESET: &str = "\x1b[0m";

pub const OK_ANSI: &str = "\x1b[38;2;78;201;160m";
pub const RUNNING_ANSI: &str = "\x1b[38;2;229;192;123m";
pub const FAILED_ANSI: &str = "\x1b[38;2;224;92;106m";
pub const PENDING_ANSI: &str = "\x1b[38;2;74;88;120m";

pub const MUTED_ANSI: &str = "\x1b[38;2;74;88;120m";
pub const TEXT_ANSI: &str = "\x1b[38;2;184;196;232m";

pub const LOG_ANSI: &str = "\x1b[38;2;255;203;107m";
pub const EXEC_ANSI: &str = "\x1b[38;2;91;156;246m";
pub const CD_ANSI: &str = "\x1b[38;2;255;203;107m";
pub const ENV_ANSI: &str = "\x1b[38;2;199;146;234m";

pub const OK: Color = Color::Rgb(78, 201, 160);
pub const RUNNING: Color = Color::Rgb(229, 192, 123);
pub const FAILED: Color = Color::Rgb(224, 92, 106);
pub const PENDING: Color = Color::Rgb(74, 88, 120);
