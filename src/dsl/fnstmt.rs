//! Parsed (unresolved) function-body statement types.
//!
//! These are pure syntax: a `FnStmt` is what the parser produces. Resolution
//! into `ResolvedFnStmt` lives in `crate::compiler::fnstmt`, so the semantic
//! layer depends on this syntax layer rather than the reverse.

use crate::dsl::{CaseArm, EnvPair, Expr, VarType};

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
    Log(Expr),
    Exec(Expr),
    Cd(Expr),
    VarDecl(VarDeclStmt),
    EnvBlock(EnvBlockStmt),
    Case(CaseStmt),
}

impl FnStmt {
    /// Invoke `f` with a mutable handle to the namespace of every variable this
    /// statement references (including `env` pairs and `case`
    /// conditions/patterns/bodies), plus each reference's span. Mirrors
    /// [`FnStmt::visit_vars`] so a normalization pass can rewrite the `self`
    /// alias throughout a function body in one place.
    pub fn visit_namespaces_mut(&mut self, f: &mut impl FnMut(&mut String, usize, usize, &str)) {
        match self {
            FnStmt::Log(value) => value.visit_namespaces_mut(f),
            FnStmt::Exec(value) => value.visit_namespaces_mut(f),
            FnStmt::Cd(value) => value.visit_namespaces_mut(f),
            FnStmt::VarDecl(s) => s.value.visit_namespaces_mut(f),
            FnStmt::EnvBlock(s) => {
                for pair in &mut s.pairs {
                    pair.value.visit_namespaces_mut(f);
                }
                for stmt in &mut s.body {
                    stmt.visit_namespaces_mut(f);
                }
            }
            FnStmt::Case(s) => {
                s.condition.visit_namespaces_mut(f);
                for arm in &mut s.scopes {
                    arm.pattern.visit_namespaces_mut(f);
                    for stmt in &mut arm.body {
                        stmt.visit_namespaces_mut(f);
                    }
                }
            }
        }
    }

    /// Invoke `f` with every variable reference this statement contains,
    /// including `env` pairs and `case` conditions/patterns/bodies, as
    /// `(name, namespace)`. Mirrors [`Expr::visit_vars`] so the var walk is
    /// defined in exactly one place per node kind.
    pub fn visit_vars(&self, f: &mut impl FnMut(&str, &str)) {
        match self {
            FnStmt::Log(value) => value.visit_vars(f),
            FnStmt::Exec(value) => value.visit_vars(f),
            FnStmt::Cd(value) => value.visit_vars(f),
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
