//! Function-body statements as enums with co-located free functions.
//!
//! `FnStmt` (parsed) and `ResolvedFnStmt` (resolved) are enums. Adding a
//! statement kind is: one enum variant + its payload struct + the per-variant
//! `validate_*` / `resolve_*` free functions below (all co-located here) +
//! one parser arm in `body.rs`. The dispatchers `validate_fn_body_stmts` and
//! `resolve_fn_body_stmts` are a single `match`, so the compiler forces every
//! variant to be handled — no central match scattered across files, and no
//! trait-object / boxed-clone indirection for AI agents to diverge on.

use crate::compiler::error::CompileError;
use crate::compiler::resolve::ShellCache;
use crate::compiler::resolve::evaluate_config_shell;
use crate::compiler::resolve::redeclaration_err;
use crate::compiler::resolve::resolve_case_pattern;
use crate::compiler::resolve::resolve_expr;
use crate::compiler::scope::{ScopeKind, ScopeStack};
use crate::compiler::types::{ResolvedCaseArm, ResolvedEnvPair};
use crate::compiler::validation::validate_expr;
use crate::dsl::{CaseStmt, EnvBlockStmt, Expr, FnStmt, VarDeclStmt, VarType};
use crate::error::{SourceFile, spanned_report_on};
use miette::Report;
use std::collections::HashMap;
use std::path::Path;

/// Per-body constants + mutable state for validating one function body.
///
/// Bundles the per-body constants (`fn_name`, `proj_name`) with the mutable
/// scope and error sink, so each statement validates itself via
/// `validate_fn_stmt(stmt, ctx)` instead of the old central match threading
/// these parameters individually.
pub(crate) struct ValidateFnCtx<'a> {
    pub fn_name: &'a str,
    pub proj_name: &'a str,
    pub scope: &'a mut ScopeStack<()>,
    pub errors: &'a mut Vec<Report>,
    pub sources: &'a HashMap<String, String>,
}

/// Per-body state for lowering one function body.
pub(crate) struct ResolveFnCtx<'a> {
    pub scope: &'a mut ScopeStack<String>,
    pub working_dir: Option<&'a Path>,
    pub sources: &'a HashMap<String, String>,
    pub shell_cache: &'a mut ShellCache,
}

// ── Resolved statement payloads ──────────────────────────────────────────────
//
// Note: the parsed counterparts (`FnStmt` and its payloads) live in
// `crate::dsl::fnstmt` — this module is the semantic (resolution) layer and
// therefore depends on the syntax layer, not the reverse.

#[derive(Debug, Clone)]
pub struct ResolvedLogStmt {
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedExecStmt {
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedCdStmt {
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedEnvBlockStmt {
    pub pairs: Vec<ResolvedEnvPair>,
    pub body: Vec<ResolvedFnStmt>,
}

#[derive(Debug, Clone)]
pub struct ResolvedCaseStmt {
    pub condition: String,
    pub scopes: Vec<ResolvedCaseArm>,
}

/// A fully resolved function-body statement, ready to execute.
#[derive(Debug, Clone)]
pub enum ResolvedFnStmt {
    Log(ResolvedLogStmt),
    Exec(ResolvedExecStmt),
    Cd(ResolvedCdStmt),
    EnvBlock(ResolvedEnvBlockStmt),
    Case(ResolvedCaseStmt),
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
) -> Result<Vec<ResolvedFnStmt>, CompileError> {
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
) -> Result<Option<ResolvedFnStmt>, CompileError> {
    match stmt {
        FnStmt::Log(s) => Ok(Some(ResolvedFnStmt::Log(ResolvedLogStmt {
            value: resolve_string_expr(&s.value, ctx)?,
        }))),
        FnStmt::Exec(s) => Ok(Some(ResolvedFnStmt::Exec(ResolvedExecStmt {
            value: resolve_string_expr(&s.value, ctx)?,
        }))),
        FnStmt::Cd(s) => Ok(Some(ResolvedFnStmt::Cd(ResolvedCdStmt {
            value: resolve_string_expr(&s.value, ctx)?,
        }))),
        FnStmt::VarDecl(s) => resolve_var_decl(s, ctx),
        FnStmt::EnvBlock(s) => resolve_env_block(s, ctx),
        FnStmt::Case(s) => resolve_case(s, ctx),
    }
}

fn resolve_string_expr(value: &Expr, ctx: &mut ResolveFnCtx) -> Result<String, CompileError> {
    resolve_expr(value, ctx.scope, ctx.sources)
}

// ── Per-variant validate ─────────────────────────────────────────────────────

fn validate_string_expr(value: &Expr, ctx: &mut ValidateFnCtx) {
    validate_expr(
        value,
        ctx.fn_name,
        &*ctx.scope,
        &mut *ctx.errors,
        ctx.proj_name,
        ctx.sources,
    );
}

fn validate_var_decl(s: &VarDeclStmt, ctx: &mut ValidateFnCtx) {
    validate_expr(
        &s.value,
        ctx.fn_name,
        &*ctx.scope,
        &mut *ctx.errors,
        ctx.proj_name,
        ctx.sources,
    );
    if ctx.scope.is_declared(&s.name) {
        let kind = ctx
            .scope
            .declaring_kind(&s.name)
            .unwrap_or(ScopeKind::Global);
        ctx.errors.push(spanned_report_on(
            format!("${} is already defined at {}", s.name, kind),
            ctx.sources,
            &s.value,
        ));
    }
    let _ = ctx.scope.declare(s.name.clone(), ());
}

fn validate_env_block(s: &EnvBlockStmt, ctx: &mut ValidateFnCtx) {
    for pair in &s.pairs {
        validate_expr(
            &pair.value,
            ctx.fn_name,
            &*ctx.scope,
            &mut *ctx.errors,
            ctx.proj_name,
            ctx.sources,
        );
    }
    validate_fn_body_stmts(&s.body, ctx);
}

fn validate_case(s: &CaseStmt, ctx: &mut ValidateFnCtx) {
    validate_expr(
        &s.condition,
        ctx.fn_name,
        &*ctx.scope,
        &mut *ctx.errors,
        ctx.proj_name,
        ctx.sources,
    );
    for arm in &s.scopes {
        arm.pattern.visit_vars(|name| {
            if !ctx.scope.is_declared(name) {
                ctx.errors.push(spanned_report_on(
                    format!(
                        "project {:?}: fn {:?}: undefined variable ${}",
                        ctx.proj_name, ctx.fn_name, name
                    ),
                    ctx.sources,
                    &arm.pattern,
                ));
            }
        });
        let guard = ctx.scope.enter(ScopeKind::Case);
        let mut arm_ctx = ValidateFnCtx {
            fn_name: ctx.fn_name,
            proj_name: ctx.proj_name,
            scope: &mut *guard.stack,
            errors: ctx.errors,
            sources: ctx.sources,
        };
        validate_fn_body_stmts(&arm.body, &mut arm_ctx);
    }
}

// ── Per-variant resolve ──────────────────────────────────────────────────────

fn resolve_var_decl(
    s: &VarDeclStmt,
    ctx: &mut ResolveFnCtx,
) -> Result<Option<ResolvedFnStmt>, CompileError> {
    let (offset, len) = s.value.offset_len();
    let source = SourceFile::from_registry(ctx.sources, s.value.source_name());
    let resolved_value = resolve_expr(&s.value, ctx.scope, ctx.sources)?;
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
    ctx.scope
        .declare(s.name.to_string(), final_value)
        .map_err(|r| redeclaration_err(r, ctx.sources, s.value.source_name(), offset, len))?;
    Ok(None)
}

fn resolve_env_block(
    s: &EnvBlockStmt,
    ctx: &mut ResolveFnCtx,
) -> Result<Option<ResolvedFnStmt>, CompileError> {
    let mut resolved_pairs = Vec::new();
    for pair in &s.pairs {
        let resolved_value = resolve_expr(&pair.value, ctx.scope, ctx.sources)?;
        resolved_pairs.push(ResolvedEnvPair {
            key: pair.key.clone(),
            value: resolved_value,
        });
    }
    let body = resolve_fn_body_stmts(&s.body, ctx)?;
    Ok(Some(ResolvedFnStmt::EnvBlock(ResolvedEnvBlockStmt {
        pairs: resolved_pairs,
        body,
    })))
}

fn resolve_case(
    s: &CaseStmt,
    ctx: &mut ResolveFnCtx,
) -> Result<Option<ResolvedFnStmt>, CompileError> {
    let condition = resolve_expr(&s.condition, ctx.scope, ctx.sources)?;
    let mut resolved_scopes = Vec::new();
    for arm in &s.scopes {
        let pattern = resolve_case_pattern(&arm.pattern, ctx.scope, ctx.sources)?;
        let guard = ctx.scope.enter(ScopeKind::Case);
        let body = resolve_fn_body_stmts(
            &arm.body,
            &mut ResolveFnCtx {
                scope: &mut *guard.stack,
                working_dir: ctx.working_dir,
                sources: ctx.sources,
                shell_cache: ctx.shell_cache,
            },
        )?;
        resolved_scopes.push(ResolvedCaseArm { pattern, body });
    }
    Ok(Some(ResolvedFnStmt::Case(ResolvedCaseStmt {
        condition,
        scopes: resolved_scopes,
    })))
}
