use crate::dsl::FnStmt;
use std::collections::HashMap;

/// How a project's dotfiles are synchronized from its git remote.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncMode {
    /// Git clone the remote to the sanctuary path.
    Clone,
    /// Skip synchronization for this project.
    Ignore,
}

impl std::fmt::Display for SyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncMode::Clone => write!(f, "clone"),
            SyncMode::Ignore => write!(f, "ignore"),
        }
    }
}

/// A compiled project block from a kiru config file.
#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub url: String,
    pub dir: String,
    pub sync: SyncMode,
    pub include_file: Option<String>,
    pub branch: Option<String>,
    pub vars: HashMap<String, String>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}

/// The top-level compiled result of a kiru config file.
#[derive(Debug, Clone)]
pub struct Sanctuary {
    pub sanctuary_path: String,
    pub projects: HashMap<String, Project>,
    pub vars: HashMap<String, String>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}
