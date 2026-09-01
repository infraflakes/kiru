use super::load_config;
use super::pager;
use crate::cli::kiru_toml;
use crate::exec::colors::{BOLD, BOLD_CYAN, CYAN, GRAY_ANSI, RESET, YELLOW};
use crate::ir::{Ir, Project};
use std::path::PathBuf;

macro_rules! style {
    ($code:expr, $($arg:tt)*) => {
        format!("{}{}{}", $code, format_args!($($arg)*), RESET)
    };
}

pub(crate) fn run_status_command(
    config_arg: Option<PathBuf>,
    kirufile_arg: Option<PathBuf>,
) -> Result<(), String> {
    let config = load_config(kirufile_arg)?;
    let toml = kiru_toml::load_kiru_toml_at(&super::get_toml_path(config_arg)).ok();
    let rendered_status_tree = format_config_as_tree(&config, toml.as_ref());
    pager::display_output_through_pager(&rendered_status_tree)?;
    Ok(())
}

/// Render the whole config (projects + runs) as an indented tree suitable for
/// the pager. Each project lists its functions; each run lists its chain of
/// `project::function` references.
fn format_config_as_tree(config: &Ir, toml: Option<&kiru_toml::KiruToml>) -> String {
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
            let repo = toml.and_then(|t| t.repos.iter().find(|r| r.name == *name));
            draw_project(&mut formatted_output, name, project, repo, is_last_project);
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
                let stage_connector = if is_last_stage { "└" } else { "├" };

                // Join multiple calls in a chain with " => " on a single line.
                let chain_display: Vec<String> = stage.iter().map(|call| call.fqn()).collect();
                let chain_line = chain_display.join(&style!(GRAY_ANSI, " => "));

                formatted_output.push_str(&format!(
                    "  {}  {}── {}\n",
                    run_indent,
                    style!(BOLD, "{}", stage_connector),
                    chain_line
                ));
            }
        }
    }

    formatted_output.push('\n');
    footer_bar(&mut formatted_output, config);
    formatted_output
}

/// Render a single project node with its functions and optional repo config.
fn draw_project(
    out: &mut String,
    name: &str,
    project: &Project,
    repo: Option<&kiru_toml::Repo>,
    last: bool,
) {
    let branch = if last { "└" } else { "├" };
    out.push_str(&format!(
        "  {}── {}\n",
        branch,
        style!(BOLD_CYAN, "{}", name)
    ));

    let indent = if last { "   " } else { "│  " };

    if let Some(repo) = repo {
        if !repo.url.is_empty() {
            project_field(out, indent, "url", &repo.url);
        }
        if !repo.dir.is_empty() {
            project_field(out, indent, "dir", &repo.dir);
        }
        if !repo.branch.is_empty() {
            project_field(out, indent, "branch", &repo.branch);
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
            style!(GRAY_ANSI, "none")
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
        "  -- {} projects, {} functions, {} runs --\n",
        config.projects.len(),
        fn_count,
        run_count,
    ));
}
