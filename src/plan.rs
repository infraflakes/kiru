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
use std::collections::BTreeMap;
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
/// The default sync mode is [`SyncMode::Clone`] and applies when the field is
/// omitted. Because `clone` is the default it is not a valid field value:
/// `ignore` is the only accepted name, and anything else — including `clone` —
/// is rejected by the generic invalid-value path below.
pub fn parse_sync_mode(value: &str) -> Result<SyncMode, String> {
    match value {
        "ignore" => Ok(SyncMode::Ignore),
        _ => Err(format!(
            "invalid sync value {:?} (expected: `ignore`; the repo is cloned and updated by default, so omit the field)",
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

/// Check whether a resolved condition matches a case pattern at compile time
/// or runtime.
pub fn match_case_pattern(pattern: &PlanCasePattern, condition: &str) -> bool {
    match pattern {
        PlanCasePattern::Literal(lit) => condition == lit,
        PlanCasePattern::Default => true,
    }
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
    Log(String),
    Exec(String),
    Cd(String),
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
    pub functions: BTreeMap<String, Vec<PlanStmt>>,
}

/// The final, fully resolved plan. The runner works exclusively with this type.
#[derive(Debug, Clone)]
pub struct Plan {
    pub projects: BTreeMap<String, PlanProject>,
    /// Top-level run blocks, keyed by run name. Each run is a set of chains of
    /// `namespace::function` references executed by the runner.
    pub runs: BTreeMap<String, Vec<Vec<QualifiedFnRef>>>,
}
