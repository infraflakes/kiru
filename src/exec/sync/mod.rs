//! Repository sync: clones or fast-forward-pulls declared repositories
//! into their configured directories.

pub(crate) mod clone;

pub(crate) use clone::run_sync_for_projects;
pub(crate) use clone::sync_project_with_callback;
