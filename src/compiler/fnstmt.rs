//! Function-body statements as enums with co-located free functions.
//!
//! `FnStmt` (parsed) and `PlanStmt` (resolved, in `crate::plan`) are enums.
//! Adding a statement kind is: one enum variant + its payload struct + the
//! per-variant `validate_*` / `resolve_*` free functions below (all co-located
//! here) + one parser arm in `body.rs`. The dispatchers `validate_fn_body_stmts`
//! and `resolve_fn_body_stmts` are a single `match`, so the compiler forces every
//! variant to be handled — no central match scattered across files, and no
//! trait-object / boxed-clone indirection for AI agents to diverge on.

use crate::compiler::error::CompileError;
use crate::compiler::namespaces::{
    Namespaces, resolve_case_pattern, resolve_expr, resolve_var_value,
};
use crate::dsl::{CaseStmt, EnvBlockStmt, FnStmt, VarDeclStmt};
use crate::error::{Span, spanned_report_on};
use crate::plan::{
    PlanCaseArm, PlanCaseStmt, PlanEnvBlockStmt, PlanEnvPair, PlanStmt, match_case_pattern,
};
use miette::Report;
use std::collections::HashMap;
use std::path::Path;

fn is_var_defined(namespaces: &Namespaces, ns: &str, name: &str) -> bool {
    if ns == "global" {
        return namespaces.global.contains_key(name);
    }
    namespaces.project_var_exists(ns, name)
}

/// Per-body constants + mutable state for validating one function body.
///
/// Bundles the per-body constants (`fn_name`, `proj_name`) with the namespaces
/// map and the error sink, so each statement validates itself via
/// `validate_fn_stmt(stmt, ctx)` instead of the old central match threading
/// these parameters individually.
pub(crate) struct ValidateFnCtx<'a> {
    pub fn_name: &'a str,
    pub proj_name: &'a str,
    pub namespaces: &'a Namespaces,
    pub errors: &'a mut Vec<Report>,
    pub sources: &'a HashMap<String, String>,
}

// ── Recursive dispatch helpers ───────────────────────────────────────────────

/// Validate every statement in `body` against `ctx`.
pub(crate) fn validate_fn_body_stmts(body: &[FnStmt], ctx: &mut ValidateFnCtx) {
    for stmt in body {
        validate_fn_stmt(stmt, ctx);
    }
}

/// Resolve every statement in `body`, returning the lowered list. `var`
/// declarations resolve to nothing and are therefore omitted.
pub(crate) fn resolve_fn_body_stmts(
    body: &[FnStmt],
    namespaces: &mut Namespaces,
    project: &str,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<Vec<PlanStmt>, CompileError> {
    let mut resolved = Vec::new();
    for stmt in body {
        if let Some(resolved_stmt) =
            resolve_fn_stmt(stmt, namespaces, project, working_dir, sources)?
        {
            resolved.push(resolved_stmt);
        }
    }
    Ok(resolved)
}

fn validate_fn_stmt(stmt: &FnStmt, ctx: &mut ValidateFnCtx) {
    stmt.visit_vars_spanned(&mut |name, namespace, offset, len, source_name| {
        if !is_var_defined(ctx.namespaces, namespace, name) {
            let msg = if ctx.namespaces.contains_ns(namespace) {
                format!(
                    "project {:?}: fn {:?}: undefined variable {}::{}",
                    ctx.proj_name, ctx.fn_name, namespace, name
                )
            } else {
                format!(
                    "project {:?}: fn {:?}: unknown project namespace {}",
                    ctx.proj_name, ctx.fn_name, namespace
                )
            };
            ctx.errors.push(spanned_report_on(
                msg,
                ctx.sources,
                source_name,
                offset,
                len,
            ));
        }
    });
}

fn resolve_fn_stmt(
    stmt: &FnStmt,
    namespaces: &mut Namespaces,
    project: &str,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<Option<PlanStmt>, CompileError> {
    match stmt {
        FnStmt::Log(value) => resolve_expr_stmt(value, PlanStmt::Log, namespaces, sources),
        FnStmt::Exec(value) => resolve_expr_stmt(value, PlanStmt::Exec, namespaces, sources),
        FnStmt::Cd(value) => resolve_expr_stmt(value, PlanStmt::Cd, namespaces, sources),
        FnStmt::VarDecl(s) => resolve_var_decl(s, namespaces, project, working_dir, sources),
        FnStmt::EnvBlock(s) => resolve_env_block(s, namespaces, project, working_dir, sources),
        FnStmt::Case(s) => resolve_case(s, namespaces, project, working_dir, sources),
    }
}

// ── Per-variant resolve ──────────────────────────────────────────────────────

/// Resolve a single-expression statement (`log`, `exec`, `cd`): the payload
/// is one value expression, so all three share this resolution flow and
/// differ only by their `PlanStmt` constructor.
fn resolve_expr_stmt(
    value: &crate::dsl::Expr,
    construct: fn(String) -> PlanStmt,
    namespaces: &mut Namespaces,
    sources: &HashMap<String, String>,
) -> Result<Option<PlanStmt>, CompileError> {
    Ok(Some(construct(resolve_expr(value, namespaces, sources)?)))
}

fn resolve_var_decl(
    s: &VarDeclStmt,
    namespaces: &mut Namespaces,
    project: &str,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<Option<PlanStmt>, CompileError> {
    let (offset, len) = s.value.offset_len();
    let span = Span {
        source_name: s.value.source_name(),
        offset,
        len,
        sources,
    };
    let resolved_value = resolve_expr(&s.value, namespaces, sources)?;
    let final_value = resolve_var_value(&s.var_type, &s.name, resolved_value, working_dir, &span)?;
    namespaces.set_project_var(project, &s.name, final_value);
    Ok(None)
}

fn resolve_env_block(
    s: &EnvBlockStmt,
    namespaces: &mut Namespaces,
    project: &str,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<Option<PlanStmt>, CompileError> {
    let mut resolved_pairs = Vec::new();
    for pair in &s.pairs {
        let resolved_value = resolve_expr(&pair.value, namespaces, sources)?;
        resolved_pairs.push(PlanEnvPair {
            key: pair.key.clone(),
            value: resolved_value,
        });
    }
    let body = resolve_fn_body_stmts(&s.body, namespaces, project, working_dir, sources)?;
    Ok(Some(PlanStmt::EnvBlock(PlanEnvBlockStmt {
        pairs: resolved_pairs,
        body,
    })))
}

fn resolve_case(
    s: &CaseStmt,
    namespaces: &mut Namespaces,
    project: &str,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<Option<PlanStmt>, CompileError> {
    let condition = resolve_expr(&s.condition, namespaces, sources)?;
    let mut matched = false;
    let mut resolved_scopes = Vec::new();
    for arm in &s.scopes {
        let pattern = resolve_case_pattern(&arm.pattern, namespaces, sources)?;
        if !matched && match_case_pattern(&pattern, &condition) {
            matched = true;
            let body = resolve_fn_body_stmts(&arm.body, namespaces, project, working_dir, sources)?;
            resolved_scopes.push(PlanCaseArm { pattern, body });
        } else {
            resolved_scopes.push(PlanCaseArm {
                pattern,
                body: Vec::new(),
            });
        }
    }
    Ok(Some(PlanStmt::Case(PlanCaseStmt {
        condition,
        scopes: resolved_scopes,
    })))
}
