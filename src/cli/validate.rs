use super::load_config;
use crate::compiler::Sanctuary;
use crate::runner::colors;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

macro_rules! style {
    ($code:expr, $($arg:tt)*) => {
        format!("{}{}{}", $code, format_args!($($arg)*), colors::RESET)
    };
}

pub fn run(config_arg: Option<PathBuf>) -> miette::Result<()> {
    let config = load_config(config_arg)?;
    let output = format_config(&config);
    display_output(&output)?;
    Ok(())
}

fn format_config(config: &Sanctuary) -> String {
    let mut formatted_output = String::new();
    formatted_output.push('\n');
    let box_width = 62usize;
    let label_width = 14usize;

    let is_standalone = crate::compiler::is_sanctuary_disabled();

    if !is_standalone {
        header_box(&mut formatted_output, box_width, label_width, config);
    }

    if is_standalone {
        let mut function_names: Vec<&String> = config.functions.keys().collect();
        function_names.sort_unstable();
        let mut run_names: Vec<&String> = config.runs.keys().collect();
        run_names.sort_unstable();

        draw_standalone(&mut formatted_output, &function_names, &run_names);
        formatted_output.push('\n');
    } else {
        let mut sorted_projects: Vec<(&String, &crate::compiler::Project)> =
            config.projects.iter().collect();
        sorted_projects.sort_by(|a, b| a.0.cmp(b.0));

        formatted_output.push_str(&format!(
            "\n  {}  {}\n\n",
            style!(colors::BOLD, "Projects"),
            style!(colors::YELLOW, "{}", sorted_projects.len())
        ));

        for (i, (name, project)) in sorted_projects.iter().enumerate() {
            draw_project(
                &mut formatted_output,
                name,
                project,
                i == sorted_projects.len() - 1,
            );
        }
    }

    footer_bar(&mut formatted_output, config);
    formatted_output.push('\n');
    formatted_output
}

// ── Header box ────────────────────────────────────────────

fn header_box(output: &mut String, box_width: usize, label_width: usize, config: &Sanctuary) {
    let top_border = format!("  ╭─{:=^width$}─╮", " Sanctuary ", width = box_width - 2);
    let bottom_border = format!("  ╰─{:=^width$}─╯", "", width = box_width - 2);
    output.push_str(&top_border);
    output.push('\n');

    key_value_row(
        output,
        box_width,
        label_width,
        "Sanctuary",
        &config.sanctuary_path,
        colors::CYAN,
    );

    output.push_str(&bottom_border);
    output.push('\n');
}

/// Render a key-value row inside the box.
///
/// `val_plain` is the unstyled text used for alignment calculation;
/// `val_color` is the ANSI colour code applied only to the value.
fn key_value_row(
    output: &mut String,
    box_width: usize,
    label_width: usize,
    key: &str,
    value_plain_text: &str,
    value_color_code: &str,
) {
    let interior = box_width - 2;
    let spacing_gap = 2;
    let value_visual_len = value_plain_text.chars().count();
    let visible_chars = label_width + spacing_gap + value_visual_len;
    let right_padding = interior.saturating_sub(visible_chars);

    let padded_key = format!("{:>label_width$}", key);
    let styled_key = style!(colors::GRAY, "{}", padded_key);
    let styled_value = style!(value_color_code, "{}", value_plain_text);

    output.push_str(&format!(
        "  │ {}{}{}{} │\n",
        styled_key,
        "  ",
        styled_value,
        " ".repeat(right_padding),
    ));
}

// ── Standalone pseudo-project (SANCTUARY=0) ───────────────

fn draw_standalone(out: &mut String, function_names: &[&String], run_names: &[&String]) {
    let items: &[(&str, &[&String])] = &[("fn", function_names), ("run", run_names)];

    for (label, names) in items {
        let styled_label = style!(colors::YELLOW, "{}", label);
        if names.is_empty() {
            out.push_str(&format!(
                "  {}:  {}\n",
                styled_label,
                style!(colors::GRAY, "—")
            ));
        } else {
            let count = style!(colors::GRAY, "({})", names.len());
            let joined = names
                .iter()
                .map(|name| style!(colors::BOLD, "{}", name))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("  {}:  {}  {}\n", styled_label, joined, count));
        }
    }
}

// ── Project tree ──────────────────────────────────────────

fn draw_project(out: &mut String, name: &str, project: &crate::compiler::Project, last: bool) {
    let branch = if last { "└" } else { "├" };
    out.push_str(&format!(
        "  {}── {}\n",
        branch,
        style!(colors::BOLD_CYAN, "{}", name)
    ));

    let indent = if last { "   " } else { "│  " };

    project_field(out, indent, "url", &project.url);
    project_field(out, indent, "dir", &project.dir);

    if let Some(ref branch) = project.branch {
        project_field(out, indent, "branch", branch);
    }

    project_field(out, indent, "sync", &project.sync.to_string());

    let mut project_function_names: Vec<&String> = project.functions.keys().collect();
    project_function_names.sort_unstable();
    let mut project_run_names: Vec<&String> = project.runs.keys().collect();
    project_run_names.sort_unstable();

    let items: &[(&str, &Vec<&String>)] =
        &[("fn", &project_function_names), ("run", &project_run_names)];

    for (i, (label, names)) in items.iter().enumerate() {
        let last_item = i == items.len() - 1;
        let connector = if last_item { "└" } else { "├" };
        draw_item_line(out, indent, connector, label, names);
    }

    out.push('\n');
}

fn project_field(out: &mut String, indent: &str, key: &str, value: &str) {
    out.push_str(&format!(
        "  {}  ├── {:>7}:  {}\n",
        indent,
        style!(colors::CYAN, "{}", key),
        value
    ));
}

fn draw_item_line(out: &mut String, indent: &str, connector: &str, label: &str, names: &[&String]) {
    if names.is_empty() {
        out.push_str(&format!(
            "  {}  {}── {:>7}:  {}\n",
            indent,
            connector,
            style!(colors::YELLOW, "{}", label),
            style!(colors::GRAY, "—")
        ));
    } else {
        let count = style!(colors::GRAY, "({})", names.len());
        let joined = names
            .iter()
            .map(|name| style!(colors::BOLD, "{}", name))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "  {}  {}── {:>7}:  {}  {}\n",
            indent,
            connector,
            style!(colors::YELLOW, "{}", label),
            joined,
            count
        ));
    }
}

// ── Footer ────────────────────────────────────────────────

fn footer_bar(out: &mut String, config: &Sanctuary) {
    let total_fns: usize = config
        .projects
        .values()
        .map(|project| project.functions.len())
        .sum();
    let total_runs: usize = config
        .projects
        .values()
        .map(|project| project.runs.len())
        .sum();
    let standalone_fns = config.functions.len();
    let standalone_runs = config.runs.len();

    let is_standalone = crate::compiler::is_sanctuary_disabled();
    let fn_count = total_fns + standalone_fns;
    let run_count = total_runs + standalone_runs;

    if is_standalone {
        out.push_str(&style!(
            colors::GRAY,
            "  ─ {} functions · {} runs ─\n",
            fn_count,
            run_count,
        ));
    } else {
        out.push_str(&style!(
            colors::GRAY,
            "  ─ {} projects · {} functions · {} runs ─\n",
            config.projects.len(),
            fn_count,
            run_count,
        ));
    }
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
    let (program, args) = pager_parts
        .split_first()
        .ok_or_else(|| miette::miette!("no pager command in PAGER='{}'", pager))?;

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
        if status.code().is_none() {
            std::process::exit(130);
        }
        return Err(miette::miette!(
            "pager '{}' exited with code {:?}",
            pager,
            status.code()
        ));
    }

    Ok(())
}
