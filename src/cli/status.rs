use super::load_config;
use super::pager;
use crate::dsl::ast::QualifiedFnRef;
use crate::plan::Plan;
use crate::plan::PlanProject;
use crate::runner::colors::{BOLD, BOLD_CYAN, CYAN, GRAY, RESET, YELLOW};
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

fn format_config_as_tree(config: &Plan) -> String {
    let mut formatted_output = String::new();
    formatted_output.push('\n');

    let mut sorted_projects: Vec<(&String, &PlanProject)> = config.projects.iter().collect();
    sorted_projects.sort_by(|a, b| a.0.cmp(b.0));

    let has_projects = !sorted_projects.is_empty();
    let has_runs = !config.runs.is_empty();

    if has_projects {
        formatted_output.push_str(&format!(
            "\n  {}  {}\n\n",
            style!(BOLD, "Projects"),
            style!(YELLOW, "{}", sorted_projects.len())
        ));

        for (i, (name, project)) in sorted_projects.iter().enumerate() {
            let is_last_project = i == sorted_projects.len() - 1;
            draw_project(&mut formatted_output, name, project, is_last_project);
        }
    }

    if has_runs {
        let mut sorted_runs: Vec<(&String, &Vec<Vec<QualifiedFnRef>>)> =
            config.runs.iter().collect();
        sorted_runs.sort_by(|a, b| a.0.cmp(b.0));

        formatted_output.push_str(&format!("\n  {}\n", style!(BOLD, "Runs")));

        for (run_idx, (name, chains)) in sorted_runs.iter().enumerate() {
            let is_last_run = run_idx == sorted_runs.len() - 1;
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
                    .map(|q| format!("{}::{}", q.project, q.function))
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

    project_field(out, indent, "url", &project.url);
    project_field(out, indent, "dir", &project.dir);

    if let Some(ref branch) = project.branch {
        project_field(out, indent, "branch", branch);
    }

    project_field(out, indent, "sync", &project.sync.to_string());

    let mut project_function_names: Vec<&String> = project.functions.keys().collect();
    project_function_names.sort_unstable();

    let items: &[(&str, &Vec<&String>)] = &[("fn", &project_function_names)];

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
            style!(GRAY, "—")
        ));
    } else {
        let count = style!(GRAY, "({})", names.len());
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
        GRAY,
        "  ─ {} projects · {} functions · {} runs ─\n",
        config.projects.len(),
        fn_count,
        run_count,
    ));
}
