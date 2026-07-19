//! Parsed (unresolved) function-body statement types.
//!
//! These are pure syntax: a `FnStmt` is what the parser produces. Resolution
//! into `ResolvedFnStmt` lives in `crate::compiler::fnstmt`, so the semantic
//! layer depends on this syntax layer rather than the reverse.

use crate::dsl::{CaseArm, EnvPair, Expr, VarType};

#[derive(Debug, Clone)]
pub struct LogStmt {
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct ExecStmt {
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct CdStmt {
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct VarDeclStmt {
    pub var_type: VarType,
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct EnvBlockStmt {
    pub pairs: Vec<EnvPair>,
    pub body: Vec<FnStmt>,
}

#[derive(Debug, Clone)]
pub struct CaseStmt {
    pub condition: Expr,
    pub scopes: Vec<CaseArm>,
}

/// A parsed (unresolved) function-body statement.
#[derive(Debug, Clone)]
pub enum FnStmt {
    Log(LogStmt),
    Exec(ExecStmt),
    Cd(CdStmt),
    VarDecl(VarDeclStmt),
    EnvBlock(EnvBlockStmt),
    Case(CaseStmt),
}

impl FnStmt {
    /// Invoke `f` with every variable this statement references, including the
    /// expressions inside `env` pairs and `case` conditions/patterns. The
    /// callback receives `(name, namespace)` — both always present, since every
    /// reference is written `namespace::name`. Mirrors [`Expr::visit_vars`] so
    /// the var walk is defined in exactly one place per node kind.
    pub fn visit_vars(&self, f: &mut impl FnMut(&str, &str)) {
        match self {
            FnStmt::Log(s) => s.value.visit_vars(f),
            FnStmt::Exec(s) => s.value.visit_vars(f),
            FnStmt::Cd(s) => s.value.visit_vars(f),
            FnStmt::VarDecl(s) => s.value.visit_vars(f),
            FnStmt::EnvBlock(s) => {
                for pair in &s.pairs {
                    pair.value.visit_vars(f);
                }
                for stmt in &s.body {
                    stmt.visit_vars(f);
                }
            }
            FnStmt::Case(s) => {
                s.condition.visit_vars(f);
                for arm in &s.scopes {
                    arm.pattern.visit_vars(f);
                    for stmt in &arm.body {
                        stmt.visit_vars(f);
                    }
                }
            }
        }
    }
}
