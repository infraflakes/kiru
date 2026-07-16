use crate::dsl::{Expr, FnStmt, VarType};
use std::collections::{HashMap, HashSet};

/// How a project's dotfiles are synchronized from its git remote.
#[derive(Debug, Clone, PartialEq)]
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

/// A fully resolved function-body statement with all variable references
/// substituted and `var shell` commands executed at compile time.
///
/// No `Expr`, `VarRef`, or `VarDecl` remains — every value is a flat `String`.
/// `VarDecl` nodes are dropped entirely because their bindings are inlined.
#[derive(Debug, Clone)]
pub enum ResolvedFnStmt {
    /// `log <string>` — prints the resolved string at runtime.
    Log { value: String },
    /// `exec <string>` — executes the resolved string as a shell command.
    Exec { value: String },
    /// `cd <string>` — changes working directory to the resolved path.
    Cd { value: String },
    /// `env { ... }` — scoped environment variables with a resolved body.
    EnvBlock {
        pairs: Vec<ResolvedEnvPair>,
        body: Vec<ResolvedFnStmt>,
    },
    /// `case` — condition and all pattern strings are fully resolved.
    Case {
        condition: String,
        scopes: Vec<ResolvedCaseArm>,
    },
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
    /// Canonical path of the `.kiru` file this declaration was parsed from.
    /// Carried so redeclaration diagnostics resolve against the file that
    /// actually declared the variable when a project body is merged from
    /// several files.
    pub source_name: String,
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
    /// Stores the resolved values of field-referenced project `var shell`
    /// commands, computed during the linear phase (current-dir).  These are
    /// seeded back into the scope during `resolve_with_scopes` so fields
    /// that reference them stay consistent.  Non-field-referenced vars do
    /// NOT appear here — they are resolved for the first time in the second
    /// pass with the correct project working directory.
    pub vars: HashMap<String, String>,
    /// Names of project var stmts that are referenced by at least one field
    /// expression (url/dir/sync/branch).  These must run eagerly in the linear
    /// phase (with current-dir) so field interpolation works.  The recorded
    /// linear-phase value is seeded back during re-resolution — no second exec.
    pub field_refd_vars: HashSet<String>,
    /// All variable names declared in the project body, regardless of whether
    /// they are field-referenced.  Used by validation to seed the scope so
    /// function bodies can reference project-level vars.  Populated during
    /// the linear phase from the body's `var` / `var shell` statements.
    pub declared_var_names: HashSet<String>,
    /// Minimal var-statement data for re-resolution in `resolve_with_scopes`.
    /// Non-field-referenced vars are skipped during the linear phase and
    /// resolved for the first time here with the correct working directory.
    /// Field-referenced vars carry the offset/len used for span-accurate
    /// redeclaration diagnostics when seeding.
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
