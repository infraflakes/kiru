use crate::plan::Sync;
use crate::runner;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Sync all projects via the TUI.
pub fn run_sync_command(config_arg: Option<PathBuf>) -> miette::Result<()> {
    let config = super::load_config(config_arg)?;

    let total_project_count = config.syncs.len();

    let syncs: BTreeMap<String, Sync> = config
        .syncs
        .into_iter()
        .filter(|(name, sync)| {
            let url = &sync.url;
            let dir = &sync.dir;
            let skip_reason = if url.parts.is_empty() && dir.parts.is_empty() {
                Some("missing url and dir")
            } else if url.parts.is_empty() {
                Some("missing url")
            } else if dir.parts.is_empty() {
                Some("missing dir")
            } else {
                None
            };
            if let Some(reason) = skip_reason {
                crate::error::print_diagnostic(&miette::miette!(
                    "project {:?}: {}, skipping sync",
                    name,
                    reason
                ));
                false
            } else {
                true
            }
        })
        .collect();

    if syncs.is_empty() {
        if total_project_count == 0 {
            crate::error::print_diagnostic(&miette::miette!("no projects to sync"));
            return Ok(());
        }
        return Err(miette::miette!(
            "all projects were skipped due to missing url or dir"
        ));
    }

    runner::sync::run_sync_for_projects(syncs, &config.shell)
}
