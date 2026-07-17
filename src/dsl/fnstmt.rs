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
