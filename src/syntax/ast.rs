use crate::syntax::FnStmt;
use crate::syntax::source::Template;

/// A parsed statement node in the kiru DSL.
#[derive(Debug, Clone)]
pub(crate) enum Stmt {
    /// A variable declaration (`var name = value`). The value is a template
    /// inlined at every use site; any `$(command)` inside it resolves where
    /// it is used, never here.
    Var {
        name: String,
        value: Template,
        offset: usize,
        len: usize,
    },
    /// A function definition (`fn name(params) { ... };`): a named bundle of
    /// statements, callable by name from any body and importable across
    /// files. Functions are global; a call runs in the project context of
    /// the enclosing `project(...)` block, or at the invocation context.
    Fn {
        name: String,
        params: Vec<String>,
        body: Vec<FnStmt>,
        offset: usize,
        len: usize,
    },
    /// A run block definition: `run name { statement; ... };` - an entry
    /// point whose body is an ordinary statement list. `async() { ... }`
    /// expresses concurrency; `;` always means "then".
    Run {
        name: String,
        body: Vec<FnStmt>,
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
