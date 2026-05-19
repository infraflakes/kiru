// Palette matching the Go version (old-go/internal/tui/colors.go)
#![allow(dead_code)]

use ratatui::style::Color;

// ── ANSI escape sequences (plain-text output) ────────────────

pub const RESET: &str = "\x1b[0m";

pub const OK_ANSI: &str = "\x1b[38;2;78;201;160m";
pub const RUNNING_ANSI: &str = "\x1b[38;2;229;192;123m";
pub const FAILED_ANSI: &str = "\x1b[38;2;224;92;106m";
pub const PENDING_ANSI: &str = "\x1b[38;2;74;88;120m";

pub const SEQ_ANSI: &str = "\x1b[38;2;199;146;234m";
pub const PAR_ANSI: &str = "\x1b[38;2;91;156;246m";
pub const SYNC_ANSI: &str = "\x1b[38;2;78;201;160m";

pub const MUTED_ANSI: &str = "\x1b[38;2;74;88;120m";
pub const TEXT_ANSI: &str = "\x1b[38;2;184;196;232m";
pub const TEXT_BRIGHT_ANSI: &str = "\x1b[38;2;216;226;248m";
pub const DIM_ANSI: &str = "\x1b[38;2;42;53;72m";

pub const LOG_ANSI: &str = "\x1b[38;2;255;203;107m";
pub const EXEC_ANSI: &str = "\x1b[38;2;91;156;246m";
pub const CD_ANSI: &str = "\x1b[38;2;255;203;107m";
pub const ENV_ANSI: &str = "\x1b[38;2;199;146;234m";

// ── Ratatui Color constants (TUI rendering) ─────────────────

pub const OK: Color = Color::Rgb(78, 201, 160);
pub const RUNNING: Color = Color::Rgb(229, 192, 123);
pub const FAILED: Color = Color::Rgb(224, 92, 106);
pub const PENDING: Color = Color::Rgb(74, 88, 120);

pub const SEQ: Color = Color::Rgb(199, 146, 234);
pub const PAR: Color = Color::Rgb(91, 156, 246);
pub const SYNC: Color = Color::Rgb(78, 201, 160);

pub const MUTED: Color = Color::Rgb(74, 88, 120);
pub const TEXT: Color = Color::Rgb(184, 196, 232);
pub const TEXT_BRIGHT: Color = Color::Rgb(216, 226, 248);
pub const DIM: Color = Color::Rgb(42, 53, 72);

pub const LOG: Color = Color::Rgb(255, 203, 107);
pub const EXEC: Color = Color::Rgb(91, 156, 246);
pub const CD: Color = Color::Rgb(255, 203, 107);
pub const ENV: Color = Color::Rgb(199, 146, 234);
