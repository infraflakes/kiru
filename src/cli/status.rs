//! `kiru status` renderer: shows the selected profile (source, output,
//! shell, timeout), its projects, and the compiled run blocks from the IR
//! when one has been compiled. Nothing here runs anything; the sections
//! mirror the input files so what is displayed is exactly what is
//! configured.

use super::CliError;
use super::pager;
use crate::cli::kiru_toml::ResolvedProfile;
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
    profile_arg: Option<&str>,
) -> Result<(), CliError> {
    let (profile, _) = super::resolve_selected_profile(config_arg, profile_arg)?;

    // The IR is optional: without a compiled kirufile there are simply no
    // runs to show. A malformed one is still an error, status validates it.
    let runs = match super::load_config(&profile.output) {
        Ok(config) => Some(config),
        Err(message) if message.contains("failed to read") => None,
        Err(message) => return Err(CliError::message(message)),
    };

    let rendered_status_tree = format_status_tree(&profile, runs.as_ref());
    pager::display_output_through_pager(&rendered_status_tree)
        .map_err(|e| CliError::message(format!("failed to display output: {}", e)))?;
    Ok(())
}

/// Render the whole status: the selected profile (paths and options), its
/// projects, and the run blocks when a compiled IR exists. Without one,
/// the output shows exactly what is configured.
pub(crate) fn format_status_tree(profile: &ResolvedProfile, runs: Option<&Ir>) -> String {
    let mut out = String::new();
    out.push('\n');

    draw_profile(&mut out, profile);
    draw_projects(&mut out, profile);
    if let Some(runs) = runs {
        draw_runs(&mut out, runs);
    }

    out
}

/// Draw the selected profile: its name, the resolved compile paths, and
/// the options it sets. Unset options stay invisible.
fn draw_profile(out: &mut String, profile: &ResolvedProfile) {
    out.push_str(&format!(
        "\n  {}  {}\n\n",
        style!(BOLD, "Profile"),
        style!(YELLOW, "{}", profile.name)
    ));
    let mut shown: Vec<(&str, String)> = Vec::new();
    shown.push(("source", profile.source.display().to_string()));
    shown.push(("output", profile.output.display().to_string()));
    if let Some(shell) = &profile.shell {
        shown.push(("shell", shell.clone()));
    }
    if let Some(timeout) = profile.timeout {
        shown.push(("timeout", timeout.to_string()));
    }
    let count = shown.len();
    for (index, (key, value)) in shown.iter().enumerate() {
        draw_option(out, index == count - 1, key, value);
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

/// Draw the profile's projects, in name order. Each project shows the
/// fields it actually sets.
fn draw_projects(out: &mut String, profile: &ResolvedProfile) {
    out.push_str(&format!(
        "\n  {}  {}\n\n",
        style!(BOLD, "Projects"),
        style!(YELLOW, "{}", profile.projects.len())
    ));

    let count = profile.projects.len();
    for (index, (name, project)) in profile.projects.iter().enumerate() {
        let last = index == count - 1;
        let branch = if last { "└" } else { "├" };
        out.push_str(&format!(
            "  {}── {}\n",
            branch,
            style!(BOLD_CYAN, "{}", name)
        ));
        let indent = if last { "   " } else { "│  " };

        let fields: [(&str, &str); 3] = [
            ("url", project.url.as_str()),
            ("dir", project.dir.as_str()),
            ("branch", project.branch.as_str()),
        ];
        let mut shown_fields: Vec<(&str, &str)> = fields
            .into_iter()
            .filter(|(_, value)| !value.is_empty())
            .collect();
        if project.direnv {
            shown_fields.push(("direnv", "true"));
        }
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

/// Draw the compiled run blocks. Only called when an IR exists.
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
    use crate::cli::kiru_toml;
    use std::collections::BTreeMap;

    fn sample_profile(name: &str, source: &str, output: &str) -> ResolvedProfile {
        ResolvedProfile {
            name: name.to_string(),
            source: PathBuf::from(source),
            output: PathBuf::from(output),
            shell: Some("zsh".to_string()),
            timeout: Some(300),
            projects: BTreeMap::from([(
                "kiru".to_string(),
                kiru_toml::TomlProject {
                    url: "https://github.com/infraflakes/kiru.git".to_string(),
                    dir: "~/Projects/kiru".to_string(),
                    branch: "dev".to_string(),
                    direnv: true,
                },
            )]),
        }
    }

    fn sample_runs() -> Ir {
        let mut chains = BTreeMap::new();
        chains.insert(
            "ci".to_string(),
            vec![vec![crate::ir::Call {
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
    fn tree_shows_profile_projects_and_runs() {
        let profile = sample_profile("ci", "src/main.kiru", "dist/ci/kirufile");
        let tree = format_status_tree(&profile, Some(&sample_runs()));
        assert!(tree.contains("Profile") && tree.contains("ci"), "{tree}");
        assert!(tree.contains("src/main.kiru"), "{tree}");
        assert!(tree.contains("dist/ci/kirufile"), "{tree}");
        assert!(tree.contains("shell") && tree.contains("zsh"), "{tree}");
        assert!(tree.contains("timeout") && tree.contains("300"), "{tree}");
        assert!(tree.contains("Projects"), "{tree}");
        assert!(tree.contains("kiru") && tree.contains("dev"), "{tree}");
        assert!(tree.contains("direnv") && tree.contains("true"), "{tree}");
        assert!(tree.contains("Runs"), "{tree}");
        assert!(tree.contains("ci") && tree.contains("kiru::test"), "{tree}");
        // Functions are dead display weight since `kiru fn` was removed.
        assert!(!tree.contains("fn:"), "{tree}");
        // The footer is gone.
        assert!(!tree.contains("projects,"), "{tree}");
    }

    #[test]
    fn missing_ir_renders_no_runs() {
        let profile = sample_profile("ci", "src/main.kiru", "dist/ci/kirufile");
        let tree = format_status_tree(&profile, None);
        assert!(tree.contains("Profile"), "{tree}");
        assert!(tree.contains("Projects"), "{tree}");
        // No IR: no runs section at all, no warning line.
        assert!(!tree.contains("Runs"), "{tree}");
    }
}
