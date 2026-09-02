//! Repository sync: clones or fast-forward-pulls declared repositories
//! into their configured directories.

pub(crate) mod clone;

pub(crate) use clone::RepoSync;
pub(crate) use clone::run_sync_for_projects;
