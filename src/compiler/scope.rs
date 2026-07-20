//! # Scope normalization and isolation
//!
//! kiru's variable model is deliberately isolated: a variable reference may only
//! target the current scope (`self::`), the top-level `global::` namespace, or -
//! equivalently to `self` - the current scope's own name. A project can never
//! read another project's variables. Cross-project orchestration happens solely
//! through `run` blocks (function references), never through variable reads.
//!
//! This module performs a single pass that, for every variable reference:
//!
//! 1. rewrites the `self` alias to the concrete enclosing scope name (the
//!    project name inside a `pr`, or `global` at the top level), so the rest of
//!    the compiler only ever sees real namespace names, and
//! 2. rejects any reference whose namespace is neither `self`, `global`, nor the
//!    enclosing scope - i.e. a forbidden cross-scope read.
//!
//! Run-block function references are also `self`-normalized (`self` at the top
//! level means `global`), but they are otherwise left to `validate_run_refs`,
//! which is what permits `global` to reach into projects by function.

use crate::compiler::types::UnresolvedProject;
use crate::dsl::Expr;
use crate::error::{SourceFile, spanned_report};
use std::collections::HashMap;

/// The namespace name of the top-level (global) scope.
pub(crate) const GLOBAL_SCOPE: &str = "global";

/// The scope used when validating a shared (global) function body at
/// declaration time. `self::` inside such a body is intentionally LEFT
/// symbolic (rewritten to `"self"`, i.e. unchanged) rather than frozen to
/// `global`: the function has no host yet, and `self::` is bound to the
/// applying project only when the function is `use`d. `global::` is always
/// allowed, and any other namespace is rejected. Using `"self"` as the scope
/// makes `rewrite_and_check` treat `self` as already-correct (a no-op rewrite)
/// while still permitting `global` and rejecting cross-scope reads.
pub(crate) const TEMPLATE_SCOPE: &str = "self";

/// Check one variable-reference namespace against `scope` and rewrite the `self`
/// alias in place. Illegal cross-scope references are pushed onto `errors` with
/// a spanned diagnostic; the namespace is left unchanged in that case (the
/// caller aborts before resolution once any error is collected).
pub(crate) fn rewrite_and_check(
    namespace: &mut String,
    scope: &str,
    offset: usize,
    len: usize,
    source_name: &str,
    sources: &HashMap<String, String>,
    errors: &mut Vec<miette::Report>,
) {
    if namespace == "self" {
        *namespace = scope.to_string();
        return;
    }
    if namespace == GLOBAL_SCOPE || namespace == scope {
        return;
    }
    let source = SourceFile::from_registry(sources, source_name);
    let where_ = if scope == GLOBAL_SCOPE {
        "a global variable".to_string()
    } else {
        format!("project `{}`", scope)
    };
    errors.push(spanned_report(
        format!(
            "invalid variable reference `{}::`: {} may only reference `self::` or `global::`",
            namespace, where_
        ),
        &source,
        offset,
        len,
    ));
}

/// Normalize (and check) every variable reference inside a single expression.
pub(crate) fn normalize_expr(
    expr: &mut Expr,
    scope: &str,
    sources: &HashMap<String, String>,
    errors: &mut Vec<miette::Report>,
) {
    expr.visit_namespaces_mut(&mut |namespace, offset, len, source_name| {
        rewrite_and_check(namespace, scope, offset, len, source_name, sources, errors);
    });
}

/// Normalize (and check) every variable reference in a whole project: its
/// metadata fields, body variables, and function bodies. The enclosing scope is
/// the project's own name.
pub(crate) fn normalize_project(
    project: &mut UnresolvedProject,
    sources: &HashMap<String, String>,
    errors: &mut Vec<miette::Report>,
) {
    let scope = project.name.clone();
    for field in [
        &mut project.url,
        &mut project.dir,
        &mut project.sync,
        &mut project.branch,
    ]
    .iter_mut()
    .filter_map(|field| field.as_mut())
    {
        normalize_expr(field, &scope, sources, errors);
    }
    for var_stmt in &mut project.var_stmts {
        normalize_expr(&mut var_stmt.value, &scope, sources, errors);
    }
    for fn_name in project.functions.keys().cloned().collect::<Vec<_>>() {
        let body = project.functions.get_mut(&fn_name).unwrap();
        for stmt in body {
            stmt.visit_namespaces_mut(&mut |namespace, offset, len, source_name| {
                rewrite_and_check(namespace, &scope, offset, len, source_name, sources, errors);
            });
        }
    }
}
