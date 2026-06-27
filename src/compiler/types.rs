use crate::dsl::Expr;
use crate::dsl::FnStmt;
use std::collections::HashMap;

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

/// A project block with unresolved AST (Expr) fields.
/// No string resolution has been performed — fields are raw `Expr` nodes.
#[derive(Debug, Clone)]
pub struct UnresolvedProject {
    pub name: String,
    pub url: Option<Expr>,
    pub dir: Option<Expr>,
    pub sync: Option<Expr>,
    pub branch: Option<Expr>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}

/// The pre-resolution config with unresolved AST fields.
/// Validation operates on this type so errors surface before any shell execution.
#[derive(Debug, Clone)]
pub struct UnresolvedSanctuary {
    pub sanctuary_path: Option<Expr>,
    pub projects: HashMap<String, UnresolvedProject>,
    pub functions: HashMap<String, Vec<FnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
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
/// bodies at compile time. See [`UnresolvedSanctuary`] for the pre-resolution type
/// that carries variable declarations.
#[derive(Debug, Clone)]
pub struct Sanctuary {
    pub sanctuary_path: String,
    pub projects: HashMap<String, Project>,
    pub functions: HashMap<String, Vec<ResolvedFnStmt>>,
    pub runs: HashMap<String, Vec<Vec<String>>>,
}
