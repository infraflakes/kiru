use crate::dsl::{Expr, FnStmt, VarType, ast::QualifiedFnRef};
use std::collections::{HashMap, HashSet};

/// Minimal representation of a `var` / `var shell` statement inside a project
/// body, extracted from the full `Stmt::Var` AST node to avoid cloning the
/// entire `Stmt` enum (which carries unrelated variants).
#[derive(Debug, Clone)]
pub struct ProjectVarStmt {
    pub var_type: VarType,
    pub name: String,
    pub value: Expr,
    pub offset: usize,
    pub len: usize,
}

/// A project block with unresolved AST (Expr) fields.
/// No string resolution has been performed — fields are raw `Expr` nodes.
/// `source_file` records the canonical path of the `.kiru` file that defined
/// the project, used to resolve relative `dir` paths against the source file's
/// directory at resolution time.
#[derive(Debug, Clone)]
pub struct UnresolvedProject {
    pub name: String,
    pub source_file: String,
    pub url: Option<Expr>,
    pub dir: Option<Expr>,
    pub sync: Option<Expr>,
    pub branch: Option<Expr>,
    /// All variable names declared in the project body.  Used by validation to
    /// seed the scope so function bodies can reference project-level vars.
    /// Populated during the linear phase from the body's `var` / `var shell`
    /// statements.
    pub declared_var_names: HashSet<String>,
    /// Var-statement data resolved in `resolve_with_scopes`.  Each body `var`
    /// / `var shell` is resolved once there, in the project directory, and
    /// declared into the project frame (which performs duplicate detection).
    pub var_stmts: Vec<ProjectVarStmt>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<QualifiedFnRef>>>,
}

/// The pre-resolution config with unresolved AST fields.
/// Validation operates on this type so errors surface before any shell execution.
#[derive(Debug, Clone)]
pub struct UnresolvedConfig {
    pub projects: HashMap<String, UnresolvedProject>,
    /// Full text of every source file parsed during the linear phase, keyed by
    /// canonical path. Let every diagnostic resolve the correct file for its
    /// span: a project's body is merged from several `.kiru` files, so a node's
    /// offset must be interpreted against the file that defined it, not the
    /// first file to declare `pr <name>`.
    pub source_texts: HashMap<String, String>,
}
