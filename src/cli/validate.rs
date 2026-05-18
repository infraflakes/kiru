use super::load_config_and_resolve;
use crate::config::types::Config;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const GRAY: &str = "\x1b[90m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const RESET: &str = "\x1b[0m";

macro_rules! style {
    ($code:expr, $($arg:tt)*) => {
        format!("{}{}{}", $code, format_args!($($arg)*), RESET)
    };
}

pub fn run(config_arg: Option<PathBuf>) -> miette::Result<()> {
    let cfg = load_config_and_resolve(config_arg)?;
    let output = format_config(&cfg);
    display_output(&output)?;
    Ok(())
}

fn format_config(cfg: &Config) -> String {
    let mut out = String::new();
    let box_w = 62usize;
    let label_w = 14usize;

    header_box(&mut out, box_w, label_w, cfg);

    let mut sorted: Vec<(&String, &crate::config::types::Project)> = cfg.projects.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));

    out.push_str(&format!(
        "\n  {}  {}\n\n",
        style!(BOLD, "Projects"),
        style!(YELLOW, "{}", sorted.len())
    ));

    for (i, (name, proj)) in sorted.iter().enumerate() {
        draw_project(&mut out, name, proj, i == sorted.len() - 1);
    }

    footer_bar(&mut out, cfg);
    out
}

// ── Header box ────────────────────────────────────────────

fn header_box(out: &mut String, box_w: usize, label_w: usize, cfg: &Config) {
    let top = format!("  ╭─{:=^width$}─╮", " Config ", width = box_w - 2);
    let bot = format!("  ╰─{:=^width$}─╯", "", width = box_w - 2);
    out.push_str(&top);
    out.push('\n');

    kv_row(out, box_w, label_w, "Status", "●  Valid", GREEN);
    kv_row(out, box_w, label_w, "Shell", &cfg.shell, CYAN);
    kv_row(out, box_w, label_w, "Sanctuary", &cfg.sanctuary, CYAN);

    if !cfg.vars.is_empty() {
        let v = if cfg.vars.len() == 1 {
            "1 global var".to_string()
        } else {
            format!("{} global vars", cfg.vars.len())
        };
        kv_row(out, box_w, label_w, "Vars", &v, YELLOW);
    }

    out.push_str(&bot);
    out.push('\n');
}

/// Render a key-value row inside the box.
///
/// `val_plain` is the unstyled text used for alignment calculation;
/// `val_color` is the ANSI colour code applied only to the value.
fn kv_row(
    out: &mut String,
    box_w: usize,
    label_w: usize,
    key: &str,
    val_plain: &str,
    val_color: &str,
) {
    let interior = box_w - 2;
    let gap = 2;
    let val_visual = val_plain.chars().count();
    let visible = label_w + gap + val_visual;
    let pad = interior.saturating_sub(visible);

    let key_padded = format!("{:>label_w$}", key);
    let key_styled = style!(GRAY, "{}", key_padded);
    let val_styled = style!(val_color, "{}", val_plain);

    out.push_str(&format!(
        "  │ {}{}{}{} │\n",
        key_styled,
        "  ",
        val_styled,
        " ".repeat(pad),
    ));
}

// ── Project tree ──────────────────────────────────────────

fn draw_project(out: &mut String, name: &str, proj: &crate::config::types::Project, last: bool) {
    let branch = if last { "└" } else { "├" };
    out.push_str(&format!(
        "  {}── {}\n",
        branch,
        style!(BOLD_CYAN, "{}", name)
    ));

    let indent = if last { "   " } else { "│  " };

    project_field(out, indent, "url", &proj.url);
    project_field(out, indent, "dir", &proj.dir);

    if !proj.branch.is_empty() {
        project_field(out, indent, "branch", &proj.branch);
    }

    project_field(out, indent, "sync", &proj.sync);

    if let Some(ref u) = proj.use_file {
        project_field(out, indent, "use", u);
    }

    let mut proj_vars: Vec<&String> = proj.vars.keys().collect();
    proj_vars.sort_unstable();
    let mut proj_fns: Vec<&String> = proj.functions.keys().collect();
    proj_fns.sort_unstable();
    let mut proj_seqs: Vec<&String> = proj.seqs.keys().collect();
    proj_seqs.sort_unstable();
    let mut proj_pars: Vec<&String> = proj.pars.keys().collect();
    proj_pars.sort_unstable();

    let items: &[(&str, &Vec<&String>)] = &[
        ("var", &proj_vars),
        ("fn", &proj_fns),
        ("seq", &proj_seqs),
        ("par", &proj_pars),
    ];

    for (i, (label, names)) in items.iter().enumerate() {
        let last_item = i == items.len() - 1;
        let conn = if last_item { "└" } else { "├" };
        draw_item_line(out, indent, conn, label, names);
    }

    out.push('\n');
}

fn project_field(out: &mut String, indent: &str, key: &str, value: &str) {
    out.push_str(&format!(
        "  {}  ├── {:>7}:  {}\n",
        indent,
        style!(CYAN, "{}", key),
        value
    ));
}

fn draw_item_line(out: &mut String, indent: &str, conn: &str, label: &str, names: &[&String]) {
    if names.is_empty() {
        out.push_str(&format!(
            "  {}  {}── {:>7}:  {}\n",
            indent,
            conn,
            style!(YELLOW, "{}", label),
            style!(GRAY, "—")
        ));
    } else {
        let count = style!(GRAY, "({})", names.len());
        let joined = names
            .iter()
            .map(|n| style!(BOLD, "{}", n))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "  {}  {}── {:>7}:  {}  {}\n",
            indent,
            conn,
            style!(YELLOW, "{}", label),
            joined,
            count
        ));
    }
}

// ── Footer ────────────────────────────────────────────────

fn footer_bar(out: &mut String, cfg: &Config) {
    let total_fns: usize = cfg.projects.values().map(|p| p.functions.len()).sum();
    let total_seqs: usize = cfg.projects.values().map(|p| p.seqs.len()).sum();
    let total_pars: usize = cfg.projects.values().map(|p| p.pars.len()).sum();

    out.push_str(&style!(
        GRAY,
        "  ─ {} projects · {} fns · {} seqs · {} pars · {} global vars ─\n",
        cfg.projects.len(),
        total_fns,
        total_seqs,
        total_pars,
        cfg.vars.len()
    ));
}

// ── Display / pager ───────────────────────────────────────

fn display_output(output: &str) -> miette::Result<()> {
    use std::io::IsTerminal;

    let use_pager = std::io::stdout().is_terminal()
        && crossterm::terminal::size()
            .ok()
            .is_some_and(|(_, h)| output.lines().count() > h as usize);

    if use_pager {
        pipe_to_pager(output)
    } else {
        print!("{}", output);
        Ok(())
    }
}

fn pipe_to_pager(output: &str) -> miette::Result<()> {
    let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".to_string());
    let pager_parts = shlex::split(&pager)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| miette::miette!("failed to parse PAGER: '{}'", pager))?;
    let (program, args) = pager_parts.split_first().unwrap();

    let mut cmd = Command::new(program)
        .args(args)
        .arg("-R")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| miette::miette!("failed to spawn pager '{}': {}", pager, e))?;

    if let Some(mut stdin) = cmd.stdin.take() {
        stdin
            .write_all(output.as_bytes())
            .map_err(|e| miette::miette!("failed to write to pager: {}", e))?;
    }

    let status = cmd
        .wait()
        .map_err(|e| miette::miette!("pager exited with error: {}", e))?;

    if !status.success() {
        return Err(miette::miette!(
            "pager '{}' exited with code {:?}",
            pager,
            status.code()
        ));
    }

    Ok(())
}
