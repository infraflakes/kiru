//! Parsed (unresolved) function-body statement types.
//!
//! These are pure syntax: a `FnStmt` is what the parser produces. Resolution
//! into `ResolvedFnStmt` lives in `crate::compiler::fnstmt`, so the semantic
//! layer depends on this syntax layer rather than the reverse.

use crate::dsl::{CaseArm, CasePattern, EnvPair, Expr, VarType};

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

/// A child node of a `FnStmt` that can reference variables: an `Expr` (from
/// `log`/`exec`/`cd`/`var`/`env` pairs/`case` conditions) or a `CasePattern`.
#[derive(Clone, Copy)]
enum FnStmtChildRef<'a> {
    Expr(&'a Expr),
    Pattern(&'a CasePattern),
}

/// A mutable child node of a `FnStmt` that can reference variables.
enum FnStmtChildMut<'a> {
    Expr(&'a mut Expr),
    Pattern(&'a mut CasePattern),
}

impl FnStmt {
    /// Recursively walks this statement's direct children, invoking `f` for
    /// every `Expr` and `CasePattern` node in the tree (including nested
    /// `FnStmt`s inside `env` blocks and `case` arms). The single structural
    /// skeleton shared by the immutable var walks.
    fn walk_children_ref(&self, f: &mut impl FnMut(FnStmtChildRef)) {
        match self {
            FnStmt::Log(value) | FnStmt::Exec(value) | FnStmt::Cd(value) => {
                f(FnStmtChildRef::Expr(value));
            }
            FnStmt::VarDecl(s) => f(FnStmtChildRef::Expr(&s.value)),
            FnStmt::EnvBlock(s) => {
                for pair in &s.pairs {
                    f(FnStmtChildRef::Expr(&pair.value));
                }
                for stmt in &s.body {
                    stmt.walk_children_ref(f);
                }
            }
            FnStmt::Case(s) => {
                f(FnStmtChildRef::Expr(&s.condition));
                for arm in &s.scopes {
                    f(FnStmtChildRef::Pattern(&arm.pattern));
                    for stmt in &arm.body {
                        stmt.walk_children_ref(f);
                    }
                }
            }
        }
    }

    /// The mutable counterpart of [`FnStmt::walk_children_ref`], shared by the
    /// mutable walks (namespace rewriting and span remapping).
    fn walk_children_mut(&mut self, f: &mut impl FnMut(FnStmtChildMut)) {
        match self {
            FnStmt::Log(value) | FnStmt::Exec(value) | FnStmt::Cd(value) => {
                f(FnStmtChildMut::Expr(value));
            }
            FnStmt::VarDecl(s) => f(FnStmtChildMut::Expr(&mut s.value)),
            FnStmt::EnvBlock(s) => {
                for pair in &mut s.pairs {
                    f(FnStmtChildMut::Expr(&mut pair.value));
                }
                for stmt in &mut s.body {
                    stmt.walk_children_mut(f);
                }
            }
            FnStmt::Case(s) => {
                f(FnStmtChildMut::Expr(&mut s.condition));
                for arm in &mut s.scopes {
                    f(FnStmtChildMut::Pattern(&mut arm.pattern));
                    for stmt in &mut arm.body {
                        stmt.walk_children_mut(f);
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
        self.walk_children_ref(&mut |child| match child {
            FnStmtChildRef::Expr(value) => value.visit_vars(f),
            FnStmtChildRef::Pattern(pattern) => pattern.visit_vars(f),
        });
    }

    /// Invoke `f` with every variable reference this statement contains plus
    /// its source span `(offset, len, source_name)`. Used by validation so
    /// errors point at the exact reference location.
    pub fn visit_vars_spanned(&self, f: &mut impl FnMut(&str, &str, usize, usize, &str)) {
        self.walk_children_ref(&mut |child| match child {
            FnStmtChildRef::Expr(value) => value.visit_vars_spanned(f),
            FnStmtChildRef::Pattern(pattern) => pattern.visit_vars_spanned(f),
        });
    }

    /// Invoke `f` with a mutable handle to the namespace of every variable this
    /// statement references (including `env` pairs and `case`
    /// conditions/patterns/bodies), plus each reference's span. Mirrors
    /// [`FnStmt::visit_vars`] so a normalization pass can rewrite the `self`
    /// alias throughout a function body in one place.
    pub fn visit_namespaces_mut(&mut self, f: &mut impl FnMut(&mut String, usize, usize, &str)) {
        self.walk_children_mut(&mut |child| match child {
            FnStmtChildMut::Expr(value) => value.visit_namespaces_mut(f),
            FnStmtChildMut::Pattern(pattern) => pattern.visit_namespaces_mut(f),
        });
    }

    /// Overwrite the source span (`source_name`, `offset`, `len`) on every
    /// `Expr` and `CasePattern` node in this statement tree. Used by the
    /// `use fn` handler so that errors from resolving a cloned global
    /// template point to the applying `use` statement rather than to the
    /// original global function definition.
    pub fn remap_source_span(&mut self, new_source: &str, new_offset: usize, new_len: usize) {
        self.walk_children_mut(&mut |child| match child {
            FnStmtChildMut::Expr(value) => value.remap_source_span(new_source, new_offset, new_len),
            FnStmtChildMut::Pattern(pattern) => {
                pattern.remap_source_span(new_source, new_offset, new_len);
            }
        });
    }
}
