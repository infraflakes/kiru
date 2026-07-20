use crate::dsl::{Expr, FnStmt, VarType, ast::QualifiedFnRef};
use std::collections::{BTreeMap, HashMap};

/// Minimal representation of a `var` / `var shell` statement inside a project
/// body (or at the top level), extracted from the full `Stmt` AST node to avoid
/// cloning the entire `Stmt` enum (which carries unrelated variants).
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
    /// Var-statement data resolved in the resolve pass. Each body `var` /
    /// `var shell` is resolved once there, in the project directory, and
    /// declared into the project namespace (which performs duplicate
    /// detection against the declare pass).
    pub var_stmts: Vec<ProjectVarStmt>,
    pub functions: BTreeMap<String, Vec<FnStmt>>,
}

/// The pre-resolution config with unresolved AST fields.
/// Validation operates on this type so errors surface before any shell execution.
#[derive(Debug, Clone)]
pub struct UnresolvedConfig {
    pub projects: BTreeMap<String, UnresolvedProject>,
    /// Top-level `run` blocks, keyed by run name. Each run is a set of chains of
    /// `namespace::function` references executed by the runner. `run` is global
    /// (namespacing already disambiguates the project a function belongs to),
    /// so it is no longer nested inside a `pr` body.
    pub runs: BTreeMap<String, Vec<Vec<QualifiedFnRef>>>,
    /// Full text of every source file parsed during the linear phase, keyed by
    /// canonical path. Let every diagnostic resolve the correct file for its
    /// span: a project's body is merged from several `.kiru` files, so a node's
    /// offset must be interpreted against the file that defined it, not the
    /// first file to declare `pr <name>`.
    pub source_texts: HashMap<String, String>,
}
