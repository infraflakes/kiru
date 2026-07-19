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
    Namespaces, ShellCache, evaluate_config_shell, resolve_case_pattern, resolve_expr,
};
use crate::compiler::validation::is_var_defined;
use crate::dsl::{CaseStmt, EnvBlockStmt, Expr, FnStmt, VarDeclStmt, VarType};
use crate::error::{SourceFile, spanned_report_on};
use crate::plan::{
    PlanCaseArm, PlanCaseStmt, PlanCdStmt, PlanEnvBlockStmt, PlanEnvPair, PlanExecStmt,
    PlanLogStmt, PlanStmt,
};
use miette::Report;
use std::collections::HashMap;
use std::path::Path;

/// Per-body constants + mutable state for validating one function body.
///
/// Bundles the per-body constants (`fn_name`, `proj_name`) with the namespaces
/// map and the error sink, so each statement validates itself via
/// `validate_fn_stmt(stmt, ctx)` instead of the old central match threading
/// these parameters individually. Every declared variable already lives in
/// `namespaces` (populated by the declare pass), so reference checks are a
/// single `get` lookup.
pub(crate) struct ValidateFnCtx<'a> {
    pub fn_name: &'a str,
    pub proj_name: &'a str,
    pub namespaces: &'a Namespaces,
    pub errors: &'a mut Vec<Report>,
    pub sources: &'a HashMap<String, String>,
}

/// Per-body state for lowering one function body.
pub(crate) struct ResolveFnCtx<'a> {
    pub namespaces: &'a mut Namespaces,
    pub project: &'a str,
    pub working_dir: Option<&'a Path>,
    pub sources: &'a HashMap<String, String>,
    pub shell_cache: &'a mut ShellCache,
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
    ctx: &mut ResolveFnCtx,
) -> Result<Vec<PlanStmt>, CompileError> {
    let mut resolved = Vec::new();
    for stmt in body {
        if let Some(resolved_stmt) = resolve_fn_stmt(stmt, ctx)? {
            resolved.push(resolved_stmt);
        }
    }
    Ok(resolved)
}

fn validate_fn_stmt(stmt: &FnStmt, ctx: &mut ValidateFnCtx) {
    match stmt {
        FnStmt::Log(s) => validate_string_expr(&s.value, ctx),
        FnStmt::Exec(s) => validate_string_expr(&s.value, ctx),
        FnStmt::Cd(s) => validate_string_expr(&s.value, ctx),
        FnStmt::VarDecl(s) => validate_var_decl(s, ctx),
        FnStmt::EnvBlock(s) => validate_env_block(s, ctx),
        FnStmt::Case(s) => validate_case(s, ctx),
    }
}

fn resolve_fn_stmt(
    stmt: &FnStmt,
    ctx: &mut ResolveFnCtx,
) -> Result<Option<PlanStmt>, CompileError> {
    match stmt {
        FnStmt::Log(s) => Ok(Some(PlanStmt::Log(PlanLogStmt {
            value: resolve_string_expr(&s.value, ctx)?,
        }))),
        FnStmt::Exec(s) => Ok(Some(PlanStmt::Exec(PlanExecStmt {
            value: resolve_string_expr(&s.value, ctx)?,
        }))),
        FnStmt::Cd(s) => Ok(Some(PlanStmt::Cd(PlanCdStmt {
            value: resolve_string_expr(&s.value, ctx)?,
        }))),
        FnStmt::VarDecl(s) => resolve_var_decl(s, ctx),
        FnStmt::EnvBlock(s) => resolve_env_block(s, ctx),
        FnStmt::Case(s) => resolve_case(s, ctx),
    }
}

fn resolve_string_expr(value: &Expr, ctx: &mut ResolveFnCtx) -> Result<String, CompileError> {
    resolve_expr(value, ctx.namespaces, ctx.sources)
}

// ── Per-variant validate ─────────────────────────────────────────────────────

fn validate_string_expr(value: &Expr, ctx: &mut ValidateFnCtx) {
    validate_expr(value, ctx);
}

fn validate_var_decl(s: &VarDeclStmt, ctx: &mut ValidateFnCtx) {
    validate_expr(&s.value, ctx);
}

fn validate_env_block(s: &EnvBlockStmt, ctx: &mut ValidateFnCtx) {
    for pair in &s.pairs {
        validate_expr(&pair.value, ctx);
    }
    validate_fn_body_stmts(&s.body, ctx);
}

fn validate_case(s: &CaseStmt, ctx: &mut ValidateFnCtx) {
    validate_expr(&s.condition, ctx);
    for arm in &s.scopes {
        validate_expr_pattern(&arm.pattern, ctx);
        validate_fn_body_stmts(&arm.body, ctx);
    }
}

/// Validate that every variable referenced by an `Expr` is defined in some
/// namespace (global or a known project).
fn validate_expr(value: &Expr, ctx: &mut ValidateFnCtx) {
    value.visit_vars(&mut |name: &str, namespace: &str| {
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
            ctx.errors.push(spanned_report_on(msg, ctx.sources, value));
        }
    });
}

/// Validate references inside a case pattern (literal interpolation or a
/// `$namespace::name` var-ref).
fn validate_expr_pattern(pattern: &crate::dsl::CasePattern, ctx: &mut ValidateFnCtx) {
    pattern.visit_vars(&mut |name: &str, namespace: &str| {
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
            ctx.errors
                .push(spanned_report_on(msg, ctx.sources, pattern));
        }
    });
}

// ── Per-variant resolve ──────────────────────────────────────────────────────

fn resolve_var_decl(
    s: &VarDeclStmt,
    ctx: &mut ResolveFnCtx,
) -> Result<Option<PlanStmt>, CompileError> {
    let (offset, len) = s.value.offset_len();
    let source = SourceFile::from_registry(ctx.sources, s.value.source_name());
    let resolved_value = resolve_expr(&s.value, ctx.namespaces, ctx.sources)?;
    let final_value = if s.var_type == VarType::Shell {
        evaluate_config_shell(
            &s.name,
            &resolved_value,
            ctx.working_dir,
            &source,
            offset,
            len,
            ctx.shell_cache,
        )?
    } else {
        resolved_value
    };
    // A `var` inside a case arm or env block declares into the enclosing
    // project namespace (there is no per-arm bucket). The declare pass already
    // reported an exact-duplicate error, so this overwrite is safe.
    ctx.namespaces
        .set_project_var(ctx.project, &s.name, final_value);
    Ok(None)
}

fn resolve_env_block(
    s: &EnvBlockStmt,
    ctx: &mut ResolveFnCtx,
) -> Result<Option<PlanStmt>, CompileError> {
    let mut resolved_pairs = Vec::new();
    for pair in &s.pairs {
        let resolved_value = resolve_expr(&pair.value, ctx.namespaces, ctx.sources)?;
        resolved_pairs.push(PlanEnvPair {
            key: pair.key.clone(),
            value: resolved_value,
        });
    }
    let body = resolve_fn_body_stmts(&s.body, ctx)?;
    Ok(Some(PlanStmt::EnvBlock(PlanEnvBlockStmt {
        pairs: resolved_pairs,
        body,
    })))
}

fn resolve_case(s: &CaseStmt, ctx: &mut ResolveFnCtx) -> Result<Option<PlanStmt>, CompileError> {
    let condition = resolve_expr(&s.condition, ctx.namespaces, ctx.sources)?;
    let mut resolved_scopes = Vec::new();
    for arm in &s.scopes {
        let pattern = resolve_case_pattern(&arm.pattern, ctx.namespaces, ctx.sources)?;
        // No per-arm bucket: the arm body resolves against the same project
        // namespace. Arm-local `var`s declare into the project namespace
        // (collision with a sibling arm is accepted — it was an exact
        // redeclaration and is reported during the declare pass).
        let body = resolve_fn_body_stmts(&arm.body, ctx)?;
        resolved_scopes.push(PlanCaseArm { pattern, body });
    }
    Ok(Some(PlanStmt::Case(PlanCaseStmt {
        condition,
        scopes: resolved_scopes,
    })))
}
