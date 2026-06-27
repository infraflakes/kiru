use crate::dsl::FnStmt;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum SyncMode {
    Clone,
    Ignore,
}

impl SyncMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncMode::Clone => "clone",
            SyncMode::Ignore => "ignore",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub url: String,
    pub dir: String,
    pub sync: SyncMode,
    pub include_file: Option<String>,
    pub branch: String,
    pub vars: HashMap<String, String>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}

#[derive(Debug, Clone)]
pub struct Sanctuary {
    pub sanctuary_path: String,
    pub projects: HashMap<String, Project>,
    pub vars: HashMap<String, String>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}
