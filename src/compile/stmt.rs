//! Per-variant statement compilation helpers, extracted from `compile_stmt`.

use crate::ir::Call;
use crate::syntax::Stmt;
use std::collections::BTreeMap;

use super::inline::inline_dsl_template;
use super::{CompileError, CompileState, PendingProject};

pub(super) fn compile_var_decl(
    name: &str,
    value: &crate::syntax::Template,
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut CompileState,
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
    state: &mut CompileState,
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
    state.run_blocks.insert(
        name.to_string(),
        super::PendingRunBlock {
            stages: ir_calls,
            source_name: source_name.to_string(),
            offset,
            len,
        },
    );
    Ok(())
}

pub(super) fn compile_project_body(
    name: &str,
    body: &[Stmt],
    source_name: &str,
    state: &mut CompileState,
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
                let lowered = super::inline::compile_function_body(
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
