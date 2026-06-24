use crate::dsl::FnStmt;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub url: String,
    pub dir: String,
    pub sync: String,
    pub include_file: Option<String>,
    pub branch: String,
    pub vars: HashMap<String, String>,
    pub shell_vars: HashMap<String, String>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}

#[derive(Debug, Clone)]
pub struct Sanctuary {
    pub sanctuary_path: String,
    pub projects: HashMap<String, Project>,
    pub vars: HashMap<String, String>,
    pub shell_vars: HashMap<String, String>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}
