use std::str::FromStr;

use crate::dsl::{Expr, FnStmt, VarType};
use crate::plan::QualifiedFnRef;

/// The key of a project block field (e.g., `url`, `dir`, `sync`, `branch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectField {
    Url,
    Dir,
    Sync,
    Branch,
}

impl ProjectField {
    /// The source spelling of the field key, used in diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectField::Url => "url",
            ProjectField::Dir => "dir",
            ProjectField::Sync => "sync",
            ProjectField::Branch => "branch",
        }
    }
}

impl FromStr for ProjectField {
    type Err = ();

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        match key {
            "url" => Ok(ProjectField::Url),
            "dir" => Ok(ProjectField::Dir),
            "sync" => Ok(ProjectField::Sync),
            "branch" => Ok(ProjectField::Branch),
            _ => Err(()),
        }
    }
}

/// A parsed statement node in the kiru DSL.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// A variable declaration (`var` or `var shell`).
    Var {
        var_type: VarType,
        name: String,
        value: Expr,
        offset: usize,
        len: usize,
    },
    /// A project block: `pr name [ field = value ... ] { fn/run/var ... }`.
    /// Fields (`url`, `dir`, `sync`, `branch`) are in `fields`; function,
    /// run, and var declarations are in `body`.
    Project {
        name: String,
        fields: Vec<Stmt>,
        body: Vec<Stmt>,
    },
    /// A named field inside a project block (url, dir, sync, branch).
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
        chains: Vec<Vec<QualifiedFnRef>>,
        offset: usize,
        len: usize,
    },
    /// Applies a shared (global) function to the enclosing project. Written
    /// `use name;` (or `use name as alias;`) inside a project body, it binds the
    /// global function `name` into the project as `project::name` (or
    /// `project::alias`). The project's own `var`s become the function's
    /// "metadata": `self::` inside the function resolves to the project, and the
    /// function runs with the project's `cwd`. A project may apply the same
    /// global function under several aliases, but re-applying an already-bound
    /// name is a duplicate-function error. Projects no longer declare functions
    /// inline — every function is global and applied with `use`.
    Use {
        function: String,
        alias: Option<String>,
        offset: usize,
        len: usize,
        source_name: String,
    },
}

/// A top-level item returned by the parser: either a DSL statement or an import directive.
#[derive(Debug, Clone)]
pub enum TopLevel {
    Stmt(Stmt),
    Import(Expr),
}

/// A set of parsed top-level items from a single source file, with source tracking
/// for error reporting. Items preserve source order and include both statements
/// and import directives.
#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<TopLevel>,
    pub source_name: String,
    pub source_text: String,
}

impl Program {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            source_name: String::new(),
            source_text: String::new(),
        }
    }

    pub fn new_with_source(name: String, text: String) -> Self {
        Self {
            items: Vec::new(),
            source_name: name,
            source_text: text,
        }
    }
}
