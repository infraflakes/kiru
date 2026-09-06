//! `kiru status` renderer: shows the `kiru.toml` options and repos, plus the
//! compiled run blocks from the kirufile when one exists. Nothing here runs
//! anything; the sections mirror the two input files so what is displayed is
//! exactly what is configured.

use super::CliError;
use super::pager;
use crate::cli::kiru_toml;
use crate::cli::kiru_toml::KiruToml;
use crate::exec::colors::{BOLD, BOLD_CYAN, CYAN, GRAY_ANSI, RESET, YELLOW};
use crate::ir::Ir;
use std::path::PathBuf;

macro_rules! style {
    ($code:expr, $($arg:tt)*) => {
        format!("{}{}{}", $code, format_args!($($arg)*), RESET)
    };
}

pub(crate) fn run_status_command(
    config_arg: Option<PathBuf>,
    kirufile_arg: Option<PathBuf>,
) -> Result<(), CliError> {
    let toml_path = super::get_toml_path(config_arg);
    // A missing kiru.toml means no config and no projects to show; a
    // malformed one is an error, status validates it.
    let toml = kiru_toml::load_kiru_toml_or_default(&toml_path)
        .map_err(|e| CliError::message(format!("cannot show status: {e}")))?;
    let mut toml = toml;
    kiru_toml::expand_repo_dirs(&mut toml);
    let has_toml = toml_path.exists();

    // The kirufile is optional: without one there are simply no runs to
    // show. A malformed kirufile is still an error, status validates it.
    let runs = match super::load_config(kirufile_arg) {
        Ok(config) => Some(config),
        Err(message) if message.contains("failed to read") => None,
        Err(message) => return Err(CliError::message(message)),
    };

    let rendered_status_tree = format_status_tree(has_toml.then_some(&toml), runs.as_ref());
    pager::display_output_through_pager(&rendered_status_tree)
        .map_err(|e| CliError::message(format!("failed to display output: {}", e)))?;
    Ok(())
}

/// Render the whole status: the kiru.toml options and repos when a
/// kiru.toml exists, and the run blocks when a kirufile exists. Absent
/// files render nothing, so the output shows exactly what is configured.
pub(crate) fn format_status_tree(toml: Option<&KiruToml>, runs: Option<&Ir>) -> String {
    let mut out = String::new();
    out.push('\n');

    if let Some(toml) = toml {
        draw_options(&mut out, toml);
        draw_projects(&mut out, toml);
    }
    if let Some(runs) = runs {
        draw_runs(&mut out, runs);
    }

    out
}

/// Draw the kiru.toml options. Only fields explicitly set in the file are
/// shown: unset options stay invisible.
fn draw_options(out: &mut String, toml: &KiruToml) {
    let has_any = toml.shell.is_some() || toml.timeout.is_some() || toml.direnv;
    if !has_any {
        return;
    }

    out.push_str(&format!(
        "\n  {}  {}\n\n",
        style!(BOLD, "Config"),
        style!(YELLOW, "")
    ));
    let last_index = [toml.shell.is_some(), toml.timeout.is_some(), toml.direnv]
        .iter()
        .filter(|shown| **shown)
        .count()
        - 1;
    let mut shown = 0;
    if let Some(shell) = &toml.shell {
        draw_option(out, shown == last_index, "shell", shell);
        shown += 1;
    }
    if let Some(timeout) = &toml.timeout {
        draw_option(out, shown == last_index, "timeout", &timeout.to_string());
        shown += 1;
    }
    if toml.direnv {
        draw_option(out, shown == last_index, "direnv", "true");
    }
}

fn draw_option(out: &mut String, last: bool, key: &str, value: &str) {
    let connector = if last { "└" } else { "├" };
    out.push_str(&format!(
        "  {}── {}  {}\n",
        connector,
        style!(CYAN, "{key}"),
        style!(BOLD, "{value}")
    ));
}

/// Draw the configured repositories from `kiru.toml`, in file order. Each
/// repo shows the fields it actually sets.
fn draw_projects(out: &mut String, toml: &KiruToml) {
    out.push_str(&format!(
        "\n  {}  {}\n\n",
        style!(BOLD, "Projects"),
        style!(YELLOW, "{}", toml.repos.len())
    ));

    let count = toml.repos.len();
    for (index, repo) in toml.repos.iter().enumerate() {
        let last = index == count - 1;
        let branch = if last { "└" } else { "├" };
        out.push_str(&format!(
            "  {}── {}\n",
            branch,
            style!(BOLD_CYAN, "{}", repo.name)
        ));
        let indent = if last { "   " } else { "│  " };

        let fields: [(&str, &str); 3] = [
            ("url", repo.url.as_str()),
            ("dir", repo.dir.as_str()),
            ("branch", repo.branch.as_str()),
        ];
        let shown_fields: Vec<(&str, &str)> = fields
            .into_iter()
            .filter(|(_, value)| !value.is_empty())
            .collect();
        for (field_idx, (key, value)) in shown_fields.iter().enumerate() {
            let is_last_field = field_idx == shown_fields.len() - 1;
            out.push_str(&format!(
                "  {}  {}── {:>7}:  {}\n",
                indent,
                if is_last_field { "└" } else { "├" },
                style!(CYAN, "{key}"),
                value
            ));
        }
    }
}

/// Draw the compiled run blocks. Only called when a kirufile exists.
fn draw_runs(out: &mut String, runs: &Ir) {
    out.push_str(&format!(
        "\n  {}  {}\n",
        style!(BOLD, "Runs"),
        style!(YELLOW, "{}", runs.execution_chains.len())
    ));

    let count = runs.execution_chains.len();
    for (run_idx, (name, stages)) in runs.execution_chains.iter().enumerate() {
        let is_last_run = run_idx == count - 1;
        let run_connector = if is_last_run { "└" } else { "├" };
        out.push_str(&format!(
            "  {}── {}\n",
            style!(BOLD, "{}", run_connector),
            style!(BOLD, "{}", name)
        ));

        let run_indent = if is_last_run { "   " } else { "│  " };
        let stage_count = stages.len();
        for (stage_idx, stage) in stages.iter().enumerate() {
            let is_last_stage = stage_idx == stage_count - 1;
            let stage_connector = if is_last_stage { "└" } else { "├" };

            // Join multiple calls in a chain with " => " on a single line.
            let chain_display: Vec<String> = stage.iter().map(|call| call.fqn()).collect();
            let chain_line = chain_display.join(&style!(GRAY_ANSI, " => "));

            out.push_str(&format!(
                "  {}  {}── {}\n",
                run_indent,
                style!(BOLD, "{}", stage_connector),
                chain_line
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Call, Ir};
    use std::collections::BTreeMap;

    fn sample_toml() -> KiruToml {
        KiruToml {
            shell: Some("zsh".to_string()),
            timeout: Some(300),
            direnv: true,
            repos: vec![kiru_toml::Repo {
                name: "kiru".to_string(),
                url: "https://github.com/infraflakes/kiru.git".to_string(),
                dir: "~/Projects/kiru".to_string(),
                branch: "dev".to_string(),
            }],
        }
    }

    fn sample_runs() -> Ir {
        let mut chains = BTreeMap::new();
        chains.insert(
            "ci".to_string(),
            vec![vec![Call {
                project: "kiru".to_string(),
                function: "test".to_string(),
            }]],
        );
        Ir {
            projects: BTreeMap::new(),
            execution_chains: chains,
        }
    }

    #[test]
    fn tree_shows_config_projects_and_runs() {
        let tree = format_status_tree(Some(&sample_toml()), Some(&sample_runs()));
        assert!(tree.contains("Config"), "{tree}");
        assert!(tree.contains("shell") && tree.contains("zsh"), "{tree}");
        assert!(tree.contains("timeout") && tree.contains("300"), "{tree}");
        assert!(tree.contains("direnv") && tree.contains("true"), "{tree}");
        assert!(tree.contains("Projects"), "{tree}");
        assert!(tree.contains("kiru") && tree.contains("dev"), "{tree}");
        assert!(tree.contains("Runs"), "{tree}");
        assert!(tree.contains("ci") && tree.contains("kiru::test"), "{tree}");
        // Functions are dead display weight since `kiru fn` was removed.
        assert!(!tree.contains("fn:"), "{tree}");
        // The footer is gone.
        assert!(!tree.contains("projects,"), "{tree}");
    }

    #[test]
    fn unset_options_render_nothing() {
        let mut toml = sample_toml();
        toml.shell = None;
        toml.timeout = None;
        toml.direnv = false;
        let tree = format_status_tree(Some(&toml), None);
        assert!(!tree.contains("Config"), "{tree}");
        assert!(!tree.contains("shell"), "{tree}");
        assert!(!tree.contains("timeout"), "{tree}");
        assert!(!tree.contains("direnv"), "{tree}");
        assert!(tree.contains("Projects"), "{tree}");
        // No kirufile: no runs section at all, no warning line.
        assert!(!tree.contains("Runs"), "{tree}");
    }

    #[test]
    fn missing_toml_renders_only_runs() {
        let tree = format_status_tree(None, Some(&sample_runs()));
        assert!(!tree.contains("Config"), "{tree}");
        assert!(!tree.contains("Projects"), "{tree}");
        assert!(tree.contains("Runs"), "{tree}");
        assert!(tree.contains("ci"), "{tree}");
    }
}
