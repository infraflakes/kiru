use crate::syntax::FnStmt;
use crate::syntax::source::Template;
use std::str::FromStr;

/// The key of a project block field (e.g., `url`, `dir`, `sync`, `branch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectField {
    Url,
    Dir,
    Branch,
    Sync,
}

impl ProjectField {
    /// The source spelling of the field key, used in diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectField::Url => "url",
            ProjectField::Dir => "dir",
            ProjectField::Branch => "branch",
            ProjectField::Sync => "sync",
        }
    }
}

impl FromStr for ProjectField {
    type Err = ();

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        match key {
            "url" => Ok(ProjectField::Url),
            "dir" => Ok(ProjectField::Dir),
            "branch" => Ok(ProjectField::Branch),
            "sync" => Ok(ProjectField::Sync),
            _ => Err(()),
        }
    }
}

/// A parsed statement node in the kiru DSL.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// A variable declaration (`var name = value`). Frozen at compile time
    /// when it contains no `$(command)` part; otherwise resolved at runtime.
    Var {
        name: String,
        value: Template,
        offset: usize,
        len: usize,
    },
    /// A project declaration. `sync name { ... }` declares a repo (fields only);
    /// `pr name { ... }` declares behavior (var + fn only). The compiler merges
    /// a `sync` and a `pr` of the same name into one project.
    Project {
        name: String,
        fields: Vec<Stmt>,
        body: Vec<Stmt>,
    },
    /// A field inside a `sync` block (url, dir, branch, sync).
    Field {
        key: ProjectField,
        value: Template,
        offset: usize,
        len: usize,
    },
    /// A function definition (`fn name { ... }`). Inside a `pr` it belongs to
    /// that project; at the top level it is a global function template.
    Fn {
        name: String,
        body: Vec<FnStmt>,
        offset: usize,
        len: usize,
    },
    /// A run block definition: `run name { pr::fn => pr::fn; pr::fn; }`.
    ///
    /// `calls` is an ordered list of chains. Calls joined by `=>` form one
    /// sequential chain (each runs after the previous); `;` separates chains,
    /// and the chains run concurrently with one another.
    Run {
        name: String,
        calls: Vec<Vec<Call>>,
        offset: usize,
        len: usize,
    },
    /// Shell configuration: `shell = (sh);` — the shell used for command
    /// substitution and `exec` statements. Declared at the top level.
    Shell {
        value: Template,
        offset: usize,
        len: usize,
        source_name: String,
    },
    /// Global timeout: `timeout = (30);` — the maximum seconds any single
    /// `$(cmd)` substitution may run before being aborted. Mandatory at the
    /// top level alongside `shell`. Declared at the top level.
    Timeout {
        value: Template,
        offset: usize,
        len: usize,
        source_name: String,
    },
}

/// A top-level item returned by the parser: either a DSL statement or an import directive.
#[derive(Debug, Clone)]
pub enum TopLevel {
    Stmt(Stmt),
    Import(Template),
}

/// A set of parsed top-level items from a single source file, with source tracking
/// for error reporting. Items preserve source order and include both statements
/// and import directives.
#[derive(Debug, Clone)]
pub struct Program {
    pub top_level_items: Vec<TopLevel>,
    pub source_name: String,
    pub source_text: String,
}

impl Program {
    pub fn new_with_source(name: String, text: String) -> Self {
        Self {
            top_level_items: Vec::new(),
            source_name: name,
            source_text: text,
        }
    }
}

/// A `project::function` reference inside a `run` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub project: String,
    pub function: String,
}
