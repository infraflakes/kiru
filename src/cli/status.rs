use super::load_config;
use super::pager;
use crate::exec::colors::{BOLD, BOLD_CYAN, CYAN, GRAY_ANSI, RESET, YELLOW};
use crate::ir::{Ir, Project, Sync, render_ir_literal};
use std::path::PathBuf;

macro_rules! style {
    ($code:expr, $($arg:tt)*) => {
        format!("{}{}{}", $code, format_args!($($arg)*), RESET)
    };
}

pub fn run_status_command(config_arg: Option<PathBuf>) -> miette::Result<()> {
    let config = load_config(config_arg)?;
    let rendered_status_tree = format_config_as_tree(&config);
    pager::display_output_through_pager(&rendered_status_tree)?;
    Ok(())
}

/// Render the whole config (projects + runs) as an indented tree suitable for
/// the pager. Each project lists its sync fields and functions; each run lists
/// its chain of `project::function` references.
fn format_config_as_tree(config: &Ir) -> String {
    let mut formatted_output = String::new();
    formatted_output.push('\n');

    let has_projects = !config.projects.is_empty();
    let has_runs = !config.execution_chains.is_empty();

    if has_projects {
        formatted_output.push_str(&format!(
            "\n  {}  {}\n\n",
            style!(BOLD, "Projects"),
            style!(YELLOW, "{}", config.projects.len())
        ));

        let count = config.projects.len();
        for (i, (name, project)) in config.projects.iter().enumerate() {
            let is_last_project = i == count - 1;
            let sync = config.repositories.get(name);
            draw_project(&mut formatted_output, name, project, sync, is_last_project);
        }
    }

    if has_runs {
        formatted_output.push_str(&format!("\n  {}\n", style!(BOLD, "Runs")));

        let count = config.execution_chains.len();
        for (run_idx, (name, calls)) in config.execution_chains.iter().enumerate() {
            let is_last_run = run_idx == count - 1;
            let run_connector = if is_last_run { "└" } else { "├" };
            formatted_output.push_str(&format!(
                "  {}── {}\n",
                style!(BOLD, "{}", run_connector),
                style!(BOLD, "{}", name)
            ));

            let run_indent = if is_last_run { "   " } else { "│  " };

            let stage_count = calls.len();
            for (stage_idx, stage) in calls.iter().enumerate() {
                let is_last_stage = stage_idx == stage_count - 1;
                if stage_idx > 0 {
                    formatted_output.push_str(&format!(
                        "  {}  {}── {}\n",
                        run_indent,
                        style!(BOLD, "{}", "│"),
                        style!(GRAY_ANSI, "=>")
                    ));
                }
                let stage_connector = if is_last_stage { "└" } else { "├" };
                for (call_idx, call) in stage.iter().enumerate() {
                    let is_last_call = call_idx == stage.len() - 1;
                    let call_connector = if is_last_call { stage_connector } else { "├" };
                    formatted_output.push_str(&format!(
                        "  {}  {}── {}\n",
                        run_indent,
                        style!(BOLD, "{}", call_connector),
                        call.fqn()
                    ));
                }
            }
        }
    }

    formatted_output.push('\n');
    footer_bar(&mut formatted_output, config);
    formatted_output
}

/// Render a single project node with its sync fields and functions.
fn draw_project(out: &mut String, name: &str, project: &Project, sync: Option<&Sync>, last: bool) {
    let branch = if last { "└" } else { "├" };
    out.push_str(&format!(
        "  {}── {}\n",
        branch,
        style!(BOLD_CYAN, "{}", name)
    ));

    let indent = if last { "   " } else { "│  " };

    if let Some(sync) = sync {
        if !sync.url.segments.is_empty() {
            project_field(out, indent, "url", &render_ir_literal(&sync.url));
        }
        if !sync.dir.segments.is_empty() {
            project_field(out, indent, "dir", &render_ir_literal(&sync.dir));
        }
        if !sync.branch.segments.is_empty() {
            project_field(out, indent, "branch", &render_ir_literal(&sync.branch));
        }
        if !sync.strategy.segments.is_empty() {
            project_field(out, indent, "sync", &render_ir_literal(&sync.strategy));
        }
    }

    let function_names: Vec<&String> = project.functions.keys().collect();
    draw_item_line(
        out,
        indent,
        if function_names.is_empty() {
            "└"
        } else {
            "├"
        },
        "fn",
        &function_names,
    );
}

fn project_field(out: &mut String, indent: &str, key: &str, value: &str) {
    out.push_str(&format!(
        "  {}  ├── {:>7}:  {}\n",
        indent,
        style!(CYAN, "{}", key),
        value
    ));
}

fn draw_item_line(out: &mut String, indent: &str, connector: &str, label: &str, names: &[&String]) {
    if names.is_empty() {
        out.push_str(&format!(
            "  {}  {}── {:>7}:  {}\n",
            indent,
            connector,
            style!(YELLOW, "{}", label),
            style!(GRAY_ANSI, "—")
        ));
    } else {
        let count = style!(GRAY_ANSI, "({})", names.len());
        let joined = names
            .iter()
            .map(|name| style!(BOLD, "{}", name))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "  {}  {}── {:>7}:  {}  {}\n",
            indent,
            connector,
            style!(YELLOW, "{}", label),
            joined,
            count
        ));
    }
}

fn footer_bar(out: &mut String, config: &Ir) {
    let fn_count: usize = config
        .projects
        .values()
        .map(|project| project.functions.len())
        .sum();
    let run_count = config.execution_chains.len();

    out.push_str(&style!(
        GRAY_ANSI,
        "  ─ {} projects · {} functions · {} runs ─\n",
        config.projects.len(),
        fn_count,
        run_count,
    ));
}
