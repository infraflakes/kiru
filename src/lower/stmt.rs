//! Per-variant statement compilation helpers, extracted from `compile_stmt`.

use crate::ir::Call;
use crate::syntax::{ProjectField, Stmt, Template};
use std::collections::BTreeMap;

use super::inline::inline_dsl_template;
use super::parse::render_literal;
use super::{CompileError, LoweringState, PendingProject, PendingSync};

pub(super) fn compile_shell_decl(
    value: &Template,
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut LoweringState,
) -> Result<(), CompileError> {
    let inlined = inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
    let resolved = render_literal(&inlined);
    if state.shell.is_some() {
        return Err(state.spanned(
            "duplicate shell declaration".to_string(),
            source_name,
            offset,
            len,
        ));
    }
    state.shell = Some(resolved);
    Ok(())
}

pub(super) fn compile_timeout_decl(
    value: &Template,
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut LoweringState,
) -> Result<(), CompileError> {
    let inlined = inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
    if inlined
        .parts
        .iter()
        .any(|p| matches!(p, crate::syntax::source::Part::Cmd(_)))
    {
        return Err(state.spanned(
            "timeout value must be a plain integer, not a $(command) expression".to_string(),
            source_name,
            offset,
            len,
        ));
    }
    let rendered = render_literal(&inlined);
    let seconds: u64 = rendered.trim().parse().map_err(|_| {
        state.spanned(
            format!(
                "timeout value must be a positive integer, got `{}`",
                rendered.trim()
            ),
            source_name,
            offset,
            len,
        )
    })?;
    if seconds == 0 {
        return Err(state.spanned(
            "timeout value must be greater than zero".to_string(),
            source_name,
            offset,
            len,
        ));
    }
    if state.timeout.is_some() {
        return Err(state.spanned(
            "duplicate timeout declaration".to_string(),
            source_name,
            offset,
            len,
        ));
    }
    state.timeout = Some(seconds);
    Ok(())
}

pub(super) fn compile_var_decl(
    name: &str,
    value: &Template,
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut LoweringState,
) -> Result<(), CompileError> {
    let inlined = inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
    if state.globals.contains_key(name) {
        return Err(state.spanned(
            format!("variable `{}` is already defined", name),
            source_name,
            offset,
            len,
        ));
    }
    state.globals.insert(name.to_string(), inlined);
    Ok(())
}

pub(super) fn compile_run_decl(
    name: &str,
    calls: &[Vec<crate::syntax::ast::Call>],
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut LoweringState,
) -> Result<(), CompileError> {
    if state.run_blocks.contains_key(name) {
        return Err(state.spanned(
            format!("duplicate run block: {}", name),
            source_name,
            offset,
            len,
        ));
    }
    let ir_calls: Vec<Vec<Call>> = calls
        .iter()
        .map(|chain| {
            chain
                .iter()
                .map(|c| Call {
                    project: c.project.clone(),
                    function: c.function.clone(),
                })
                .collect()
        })
        .collect();
    state.run_blocks.insert(name.to_string(), ir_calls);
    Ok(())
}

pub(super) fn compile_project_fields(
    name: &str,
    fields: &[Stmt],
    source_name: &str,
    state: &mut LoweringState,
) -> Result<(), CompileError> {
    if fields.is_empty() {
        return Ok(());
    }
    let pending = state
        .syncs
        .entry(name.to_string())
        .or_insert_with(|| PendingSync {
            url: None,
            dir: None,
            branch: None,
            strategy: None,
        });
    for field in fields {
        if let Stmt::Field {
            key,
            value,
            offset,
            len,
        } = field
        {
            let resolved =
                inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
            match key {
                ProjectField::Url => {
                    if pending.url.is_some() {
                        return Err(state.spanned(
                            "duplicate field 'url'".to_string(),
                            source_name,
                            *offset,
                            *len,
                        ));
                    }
                    pending.url = Some(resolved);
                }
                ProjectField::Dir => {
                    if pending.dir.is_some() {
                        return Err(state.spanned(
                            "duplicate field 'dir'".to_string(),
                            source_name,
                            *offset,
                            *len,
                        ));
                    }
                    pending.dir = Some(resolved);
                }
                ProjectField::Branch => {
                    if pending.branch.is_some() {
                        return Err(state.spanned(
                            "duplicate field 'branch'".to_string(),
                            source_name,
                            *offset,
                            *len,
                        ));
                    }
                    pending.branch = Some(resolved);
                }
                ProjectField::Sync => {
                    if pending.strategy.is_some() {
                        return Err(state.spanned(
                            "duplicate field 'sync'".to_string(),
                            source_name,
                            *offset,
                            *len,
                        ));
                    }
                    pending.strategy = Some(resolved);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn compile_project_body(
    name: &str,
    body: &[Stmt],
    source_name: &str,
    state: &mut LoweringState,
) -> Result<(), CompileError> {
    let pending = state
        .projects
        .entry(name.to_string())
        .or_insert_with(|| PendingProject {
            vars: BTreeMap::new(),
            functions: BTreeMap::new(),
        });

    let mut scope = state.globals.clone();
    for (k, v) in &pending.vars {
        scope.insert(k.clone(), v.clone());
    }

    for stmt in body {
        match stmt {
            Stmt::Var {
                name: var_name,
                value,
                offset,
                len,
            } => {
                let resolved =
                    inline_dsl_template(value, &scope, &state.source_texts, source_name)?;
                if pending.vars.contains_key(var_name) {
                    return Err(state.spanned(
                        format!(
                            "variable `{}` is already defined in project `{}`",
                            var_name, name
                        ),
                        source_name,
                        *offset,
                        *len,
                    ));
                }
                pending.vars.insert(var_name.clone(), resolved.clone());
                scope.insert(var_name.clone(), resolved);
            }
            Stmt::Fn {
                name: fn_name,
                body: fn_body,
                offset,
                len,
            } => {
                if pending.functions.contains_key(fn_name) {
                    return Err(state.spanned(
                        format!("duplicate function `{}` in project `{}`", fn_name, name),
                        source_name,
                        *offset,
                        *len,
                    ));
                }
                let lowered = super::inline::lower_function_body(
                    fn_body,
                    &scope,
                    &state.source_texts,
                    source_name,
                )?;
                pending.functions.insert(fn_name.clone(), lowered);
            }
            _ => {}
        }
    }
    Ok(())
}
