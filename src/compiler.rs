//! # Compiler Pipeline
//!
//! Kiru compiles its DSL in three stages. Parsing is a separate front-end that
//! feeds the compiler; runtime is a separate back-end that consumes its output.
//!
//! The compiler's output is a [`Config`] — a fully lowered configuration where
//! every variable reference has been substituted and function bodies contain only
//! pure [`ResolvedFnStmt`]s (no `Expr` or `VarDecl` nodes remain).  Structural
//! metadata like project fields and `Run` declarations are collected and passed
//! through without transformation — only function bodies undergo full lowering.
//!
//! 1. **Linear processing** (`compile::resolve_linear`) — AST items are walked in
//!    source order. Imported files are resolved recursively (with cycle detection).
//!    `var` bindings (including compile-time shell arithmetic via
//!    `shell::execute_shell_variable`) are evaluated and accumulated into scopes.
//!    Project, function, and run declarations are collected into an intermediate
//!    representation.
//!
//! 2. **Validation** (`validation::validate_configuration`) — structural constraints
//!    are checked against the pre-built variable scopes: run-to-function references
//!    and undefined variable references within function bodies.
//!
//! 3. **Resolution** (`resolve::resolve_with_scopes`) — all remaining variable
//!    references (standalone `$var` in expressions; `` `${var}` `` interpolation
//!    inside backtick strings) are substituted purely against the scopes built in
//!    step 1.  Function bodies are lowered to [`ResolvedFnStmt`]s — no `Expr` or
//!    `VarDecl` nodes remain.  Project fields (url, dir, sync, branch) are resolved.
//!    Run declarations pass through unchanged.

pub(crate) mod compile;
pub(crate) mod error;
pub(crate) mod fnstmt;
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
pub use fnstmt::*;
pub use types::{Config, Project, ResolvedCasePattern, ResolvedEnvPair, SyncMode};

#[cfg(test)]
pub(crate) mod test_support;
