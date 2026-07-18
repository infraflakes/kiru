use super::load_config;
use super::pager;
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

    formatted_output.push('\n');
    footer_bar(&mut formatted_output, config);
    formatted_output.push('\n');
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
    let mut project_run_names: Vec<&String> = project.runs.keys().collect();
    project_run_names.sort_unstable();

    let items: &[(&str, &Vec<&String>)] =
        &[("fn", &project_function_names), ("run", &project_run_names)];

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
    let run_count: usize = config
        .projects
        .values()
        .map(|project| project.runs.len())
        .sum();

    out.push_str(&style!(
        GRAY,
        "  ─ {} projects · {} functions · {} runs ─\n",
        config.projects.len(),
        fn_count,
        run_count,
    ));
}
