//! # Compiler Pipeline
//!
//! Kiru compiles its DSL in three stages. Parsing is a separate front-end that
//! feeds the compiler; runtime is a separate back-end that consumes its output.
//!
//! The compiler's output is a [`crate::plan::Plan`] — a fully lowered
//! configuration where every variable reference has been substituted and
//! function bodies contain only pure [`crate::plan::PlanStmt`]s (no `Expr` or
//! `VarDecl` nodes remain).  Structural metadata like project fields and `Run`
//! declarations are collected and passed through without transformation — only
//! function bodies undergo full lowering. The runner, sync driver, and CLI
//! consume `crate::plan` and never reach back into this module.
//!
//! 1. **Linear processing** (`compile::resolve_linear`) — AST items are walked in
//!    source order. Imported files are resolved recursively (with cycle detection).
//!    Globals are resolved as they are encountered: a `var string` value is
//!    substituted, and a `var shell` global is executed live at its declaration
//!    point so a later `import` path (which is also loaded during this pass) can
//!    read its real output. Project, function, and run declarations are collected
//!    into an intermediate representation, with project variable names declared
//!    into the namespaces map for later reference checks.
//!
//! 2. **Validation** (`validation::validate_configuration`) — structural constraints
//!    are checked against the namespaces map: run-to-function references and
//!    undefined variable references within function bodies.
//!
//! 3. **Resolution** (`resolve::resolve_config`) — projects are resolved in
//!    topological order. Each project/function `var shell` command is executed
//!    (live, no caching), all remaining variable references (standalone `$var`;
//!    `` `${var}` `` interpolation inside backtick strings) are substituted, and
//!    function bodies are lowered to [`crate::plan::PlanStmt`]s — no `Expr` or
//!    `VarDecl` nodes remain. Project fields (url, dir, sync, branch) are
//!    resolved. Run declarations pass through unchanged.

pub(crate) mod compile;
pub(crate) mod error;
pub(crate) mod fnstmt;
pub(crate) mod namespaces;
pub(crate) mod resolve;
pub(crate) mod scope;
pub(crate) mod types;
pub(crate) mod validation;

/// Run the full pipeline: parse, merge, resolve includes, validate, and resolve.
pub use compile::compile_and_resolve;
/// Lightweight compilation: parse, resolve vars, resolve project fields.
/// Skips validation and function body lowering.  Used by `kiru sync`.
pub use compile::parse_projects_metadata;
pub use error::CompileError;

#[cfg(test)]
pub(crate) mod test_support;
