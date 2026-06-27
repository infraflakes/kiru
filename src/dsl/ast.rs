use crate::dsl::{Expr, FnStmt, VarType};

/// The key of a project block field (e.g., `url`, `dir`, `sync`, `include`, `branch`).
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectField {
    Url,
    Dir,
    Sync,
    Include,
    Branch,
}

/// A parsed statement node in the kiru DSL.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// The top-level `sanctuary` declaration.
    Sanctuary { value: Expr },
    /// A variable declaration (`var` or `var shell`).
    Var {
        var_type: VarType,
        name: String,
        value: Expr,
        offset: usize,
        len: usize,
    },
    /// A project block definition.
    Project {
        name: String,
        body: Vec<Stmt>,
        offset: usize,
        len: usize,
    },
    /// A named field inside a project block (url, dir, sync, include, branch).
    Field {
        key: ProjectField,
        value: Expr,
        offset: usize,
        len: usize,
    },
    /// A function definition (`fn ... { ... }`).
    Fn {
        name: String,
        body: Vec<FnStmt>,
        offset: usize,
        len: usize,
    },
    /// A run block definition (`run ... { ... }`).
    Run {
        name: String,
        chains: Vec<Vec<String>>,
        offset: usize,
        len: usize,
    },
}

/// A top-level item returned by the parser: either a DSL statement or an import directive.
#[derive(Debug, Clone)]
pub enum TopLevel {
    Stmt(Stmt),
    Import(Expr),
}

/// A set of parsed statements from a single source file, with source tracking for error reporting.
#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub source_name: String,
    pub source_text: String,
}

impl Program {
    pub fn new() -> Self {
        Self {
            stmts: Vec::new(),
            source_name: String::new(),
            source_text: String::new(),
        }
    }

    pub fn set_source(&mut self, name: String, text: String) {
        self.source_name = name;
        self.source_text = text;
    }
}
