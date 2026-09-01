use crate::syntax::FnStmt;
use crate::syntax::source::Template;

/// A parsed statement node in the kiru DSL.
#[derive(Debug, Clone)]
pub(crate) enum Stmt {
    /// A variable declaration (`var name = value`). Frozen at compile time
    /// when it contains no `$(command)` part; otherwise resolved at runtime.
    Var {
        name: String,
        value: Template,
        offset: usize,
        len: usize,
    },
    /// A project declaration: `pr name { var; fn; }`. Contains behavioral
    /// definitions only (repo config lives in `kiru.toml`).
    Project { name: String, body: Vec<Stmt> },
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
}

/// A top-level item returned by the parser: either a DSL statement or an import directive.
#[derive(Debug, Clone)]
pub(crate) enum TopLevel {
    Stmt(Stmt),
    Import(Template),
}

/// A set of parsed top-level items from a single source file, with source tracking
/// for error reporting. Items preserve source order and include both statements
/// and import directives.
#[derive(Debug, Clone)]
pub(crate) struct Program {
    pub(crate) top_level_items: Vec<TopLevel>,
    pub(crate) source_name: String,
    pub(crate) source_text: String,
}

impl Program {
    pub(crate) fn new_with_source(name: String, text: String) -> Self {
        Self {
            top_level_items: Vec::new(),
            source_name: name,
            source_text: text,
        }
    }
}

/// A `project::function` reference inside a `run` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    pub(crate) project: String,
    pub(crate) function: String,
}
