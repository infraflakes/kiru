use crate::dsl::{Expr, FnStmt, VarType};

/// The key of a project block field (e.g., `url`, `dir`, `sync`, `branch`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectField {
    Url,
    Dir,
    Sync,
    Branch,
}

/// A function reference that may be qualified by a project namespace.
///
/// `QualifiedFnRef { project: None, function: "build" }` is an unqualified
/// reference resolved within the current project; `project: Some("nix")` is a
/// cross-project reference like `nix::build`, executed under `nix`'s `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedFnRef {
    pub project: Option<String>,
    pub function: String,
}

impl QualifiedFnRef {
    /// Convenience constructor for an unqualified (current-project) reference.
    #[allow(dead_code)]
    pub fn unqualified(function: impl Into<String>) -> Self {
        QualifiedFnRef {
            project: None,
            function: function.into(),
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
        offset: usize,
        len: usize,
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

    pub fn set_source(&mut self, name: String, text: String) {
        self.source_name = name;
        self.source_text = text;
    }
}
