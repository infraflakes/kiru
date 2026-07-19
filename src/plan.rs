//! The execution plan: the compiler's only outward contract.
//!
//! Kiru lowers a `.kiru` config into a [`Plan`] — every `Expr` has been
//! substituted and every `var shell` already evaluated (see the ConfigEval
//! phase in `crate::compiler`). The runner, sync driver, and CLI all consume
//! `Plan` and never reach back into the `compiler` module. This is the hard
//! boundary: adding a compiler-internal type cannot leak into execution.
//!
//! Everything is a resolved `String`. There is no type or operator system — the
//! DSL is an IaC task runner, not a general-purpose language.

use crate::dsl::ast::QualifiedFnRef;
use std::collections::HashMap;
use std::fmt;

/// How a project's dotfiles are synchronized from its git remote.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SyncMode {
    /// Git clone the remote to the sanctuary path.
    Clone,
    /// Skip synchronization for this project.
    Ignore,
}

impl fmt::Display for SyncMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
pub struct PlanEnvPair {
    pub key: String,
    pub value: String,
}

/// A pattern arm inside a resolved `case` block.
/// `VarRef` is flattened to `Literal`; only `Default` survives as-is.
#[derive(Debug, Clone)]
pub enum PlanCasePattern {
    Literal(String),
    Default,
}

/// A single arm of a resolved `case` block.
#[derive(Debug, Clone)]
pub struct PlanCaseArm {
    pub pattern: PlanCasePattern,
    pub body: Vec<PlanStmt>,
}

/// Resolved `log` statement payload.
#[derive(Debug, Clone)]
pub struct PlanLogStmt {
    pub value: String,
}

/// Resolved `exec` statement payload.
#[derive(Debug, Clone)]
pub struct PlanExecStmt {
    pub value: String,
}

/// Resolved `cd` statement payload.
#[derive(Debug, Clone)]
pub struct PlanCdStmt {
    pub value: String,
}

/// Resolved `env` block payload.
#[derive(Debug, Clone)]
pub struct PlanEnvBlockStmt {
    pub pairs: Vec<PlanEnvPair>,
    pub body: Vec<PlanStmt>,
}

/// Resolved `case` block payload.
#[derive(Debug, Clone)]
pub struct PlanCaseStmt {
    pub condition: String,
    pub scopes: Vec<PlanCaseArm>,
}

/// A fully resolved function-body statement, ready to execute.
#[derive(Debug, Clone)]
pub enum PlanStmt {
    Log(PlanLogStmt),
    Exec(PlanExecStmt),
    Cd(PlanCdStmt),
    EnvBlock(PlanEnvBlockStmt),
    Case(PlanCaseStmt),
}

/// A fully compiled project block with all function bodies resolved to
/// concrete strings. Produced by the compiler; consumed by the runner and
/// sync driver. `vars` are absent — all variables were inlined at compile time.
#[derive(Debug, Clone)]
pub struct PlanProject {
    pub url: String,
    pub dir: String,
    pub sync: SyncMode,
    pub branch: Option<String>,
    pub functions: HashMap<String, Vec<PlanStmt>>,
    /// Named run blocks. Each inner `Vec<String>` is a sequential chain built
    /// from `=>` separators; the outer `Vec` runs those chains in parallel
    /// (one per `;` separator). This already encodes `;`/=>` semantics, so no
    /// separate orchestration enum is needed.
    pub runs: HashMap<String, Vec<Vec<QualifiedFnRef>>>,
}

/// The final, fully resolved plan. The runner works exclusively with this type.
#[derive(Debug, Clone)]
pub struct Plan {
    pub projects: HashMap<String, PlanProject>,
}
