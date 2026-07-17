use crate::compiler::fnstmt::ResolvedFnStmt;
use crate::dsl::{Expr, FnStmt, VarType};
use std::collections::{HashMap, HashSet};

/// How a project's dotfiles are synchronized from its git remote.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SyncMode {
    /// Git clone the remote to the sanctuary path.
    Clone,
    /// Skip synchronization for this project.
    Ignore,
}

impl std::fmt::Display for SyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncMode::Clone => write!(f, "clone"),
            SyncMode::Ignore => write!(f, "ignore"),
        }
    }
}

/// Parse a `sync = <name>` string into a `SyncMode`.
///
/// The set of accepted names is tiny (clone / ignore), so a direct
/// `match` is simpler and more readable than a lookup table. Unknown
/// names produce a diagnostic listing the accepted names.
pub fn parse_sync_mode(value: &str) -> Result<SyncMode, String> {
    match value {
        "clone" => Ok(SyncMode::Clone),
        "ignore" => Ok(SyncMode::Ignore),
        _ => Err(format!(
            "invalid sync value {:?} (expected one of: clone, ignore)",
            value
        )),
    }
}

/// A fully resolved environment variable pair for `env` blocks.
#[derive(Debug, Clone)]
pub struct ResolvedEnvPair {
    pub key: String,
    pub value: String,
}

/// A pattern arm inside a resolved `case` block.
/// `VarRef` is flattened to `Literal`; only `Default` survives as-is.
#[derive(Debug, Clone)]
pub enum ResolvedCasePattern {
    Literal(String),
    Default,
}

/// A single arm of a resolved `case` block.
#[derive(Debug, Clone)]
pub struct ResolvedCaseArm {
    pub pattern: ResolvedCasePattern,
    pub body: Vec<ResolvedFnStmt>,
}

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
    pub runs: HashMap<String, Vec<Vec<String>>>,
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

/// A fully compiled project block with all function bodies resolved to concrete strings.
/// Produced by the resolve phase; consumed by the runner.
///
/// `vars` is absent from this type — all variables have been inlined into function
/// bodies at compile time. The runner never performs variable lookups.
#[derive(Debug, Clone)]
pub struct Project {
    pub url: String,
    pub dir: String,
    pub sync: SyncMode,
    pub branch: Option<String>,
    pub functions: HashMap<String, Vec<ResolvedFnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}

/// The final, fully resolved config with all `Expr` nodes substituted and
/// `var shell` commands executed. The runner works exclusively with this type.
///
/// `vars` is absent from this type — all variables have been inlined into function
/// bodies at compile time. See [`UnresolvedConfig`] for the pre-resolution type
/// that carries variable declarations.
#[derive(Debug, Clone)]
pub struct Config {
    pub projects: HashMap<String, Project>,
}
