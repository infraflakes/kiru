use crate::dsl::ast::FnStmt;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub url: String,
    pub dir: String,
    pub sync: String,
    pub use_file: Option<String>,
    pub branch: String,
    pub vars: HashMap<String, String>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub seqs: HashMap<String, Vec<String>>,
    pub pars: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub shell: String,
    pub sanctuary: String,
    pub projects: HashMap<String, Project>,
    pub vars: HashMap<String, String>,
}
