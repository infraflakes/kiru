use super::load_config;
use super::pager;
use crate::plan::Plan;
use crate::plan::PlanProject;
use crate::runner::colors::{BOLD, BOLD_CYAN, CYAN, GRAY_ANSI, RESET, YELLOW};
use std::path::PathBuf;

macro_rules! style {
    ($code:expr, $($arg:tt)*) => {
        format!("{}{}{}", $code, format_args!($($arg)*), RESET)
    };
}

pub fn run_status_command(config_arg: Option<PathBuf>) -> miette::Result<()> {
    let config = load_config(config_arg)?;
    let output = format_config_as_tree(&config);
    pager::display_output_through_pager(&output)?;
    Ok(())
}

/// Render the whole config (projects + runs) as an indented tree suitable for
/// the pager. Each project lists its fields and functions; each run lists its
/// chains of `namespace::function` references.
fn format_config_as_tree(config: &Plan) -> String {
    let mut formatted_output = String::new();
    formatted_output.push('\n');

    let has_projects = !config.projects.is_empty();
    let has_runs = !config.runs.is_empty();

    if has_projects {
        formatted_output.push_str(&format!(
            "\n  {}  {}\n\n",
            style!(BOLD, "Projects"),
            style!(YELLOW, "{}", config.projects.len())
        ));

        for (i, (name, project)) in config.projects.iter().enumerate() {
            let is_last_project = i == config.projects.len() - 1;
            draw_project(&mut formatted_output, name, project, is_last_project);
        }
    }

    if has_runs {
        formatted_output.push_str(&format!("\n  {}\n", style!(BOLD, "Runs")));

        for (run_idx, (name, chains)) in config.runs.iter().enumerate() {
            let is_last_run = run_idx == config.runs.len() - 1;
            let run_connector = if is_last_run { "└" } else { "├" };
            formatted_output.push_str(&format!(
                "  {}── {}\n",
                style!(BOLD, "{}", run_connector),
                style!(BOLD, "{}", name)
            ));

            let run_indent = if is_last_run { "   " } else { "│  " };

            for (chain_idx, chain) in chains.iter().enumerate() {
                let is_last_chain = chain_idx == chains.len() - 1;
                let chain_connector = if is_last_chain { "└" } else { "├" };

                let chain_str = chain
                    .iter()
                    .map(crate::plan::QualifiedFnRef::fqn)
                    .collect::<Vec<_>>()
                    .join(" => ");

                formatted_output.push_str(&format!(
                    "  {}  {}── {}\n",
                    run_indent,
                    style!(BOLD, "{}", chain_connector),
                    chain_str
                ));
            }
        }
    }

    formatted_output.push('\n');
    footer_bar(&mut formatted_output, config);
    formatted_output
}

/// Render a single project node with its fields and functions/runs.
fn draw_project(out: &mut String, name: &str, project: &PlanProject, last: bool) {
    let branch = if last { "└" } else { "├" };
    out.push_str(&format!(
        "  {}── {}\n",
        branch,
        style!(BOLD_CYAN, "{}", name)
    ));

    let indent = if last { "   " } else { "│  " };

    let sync_mode = project.sync.to_string();
    let fields: [(&str, Option<&str>); 4] = [
        ("url", Some(&project.url)),
        ("dir", Some(&project.dir)),
        ("branch", project.branch.as_deref()),
        ("sync", Some(&sync_mode)),
    ];
    for (key, value) in fields {
        if let Some(value) = value {
            project_field(out, indent, key, value);
        }
    }

    let project_function_names: Vec<&String> = project.functions.keys().collect();

    let items: &[(&str, &[&String])] = &[("fn", &project_function_names)];

    for (i, (label, names)) in items.iter().enumerate() {
        let last_item = i == items.len() - 1;
        let connector = if last_item { "└" } else { "├" };
        draw_item_line(out, indent, connector, label, names);
    }
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

fn footer_bar(out: &mut String, config: &Plan) {
    let fn_count: usize = config
        .projects
        .values()
        .map(|project| project.functions.len())
        .sum();
    let run_count = config.runs.len();

    out.push_str(&style!(
        GRAY_ANSI,
        "  ─ {} projects · {} functions · {} runs ─\n",
        config.projects.len(),
        fn_count,
        run_count,
    ));
}
