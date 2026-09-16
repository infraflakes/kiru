//! Per-variant statement compilation helpers, extracted from `compile_stmt`.

use crate::ir::Call;
use crate::syntax::Stmt;
use std::collections::BTreeMap;

use super::inline::{FnResolver, compile_fn_stmts, inline_dsl_template};
use super::{CompileError, CompileState, PendingFn, PendingProject};

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

    // Pass 1: inline vars in declaration order (a var may reference the ones
    // before it) and collect function bodies as pure AST, wherever they
    // appear, so any function can call any sibling regardless of position.
    // A project-body call `name();` binds a global function template into
    // the project under that name: the carbon-copy lowering resolves it
    // against this project's vars, so run blocks reach it as
    // `project::name`.
    let mut fn_decls: BTreeMap<String, PendingFn> = BTreeMap::new();
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
                if fn_decls.contains_key(fn_name) {
                    return Err(state.spanned(
                        format!("duplicate function `{}` in project `{}`", fn_name, name),
                        source_name,
                        *offset,
                        *len,
                    ));
                }
                fn_decls.insert(
                    fn_name.clone(),
                    PendingFn {
                        body: fn_body.clone(),
                        source_name: source_name.to_string(),
                    },
                );
            }
            Stmt::Call {
                name: fn_name,
                offset,
                len,
            } => {
                if fn_decls.contains_key(fn_name) {
                    return Err(state.spanned(
                        format!("duplicate function `{}` in project `{}`", fn_name, name),
                        source_name,
                        *offset,
                        *len,
                    ));
                }
                let Some(global_fn) = state.global_functions.get(fn_name) else {
                    return Err(state.spanned(
                        format!("undefined function: `{fn_name}` (not a global function)"),
                        source_name,
                        *offset,
                        *len,
                    ));
                };
                fn_decls.insert(
                    fn_name.clone(),
                    PendingFn {
                        body: global_fn.body.clone(),
                        source_name: global_fn.source_name.clone(),
                    },
                );
            }
            _ => {}
        }
    }

    // Pass 2: lower every collected body. A call to a sibling resolves
    // against the complete `fn_decls` map and a call to a global function
    // against the pre-collected `state.global_functions`; both are spliced
    // in carbon-copy at the call site.
    let resolver = FnResolver {
        project_functions: &fn_decls,
        global_functions: &state.global_functions,
    };
    for (fn_name, entry) in &fn_decls {
        let mut cycle_stack = vec![fn_name.clone()];
        let mut local_scope = scope.clone();
        let lowered = compile_fn_stmts(
            &entry.body,
            &mut local_scope,
            &state.source_texts,
            &entry.source_name,
            name,
            &resolver,
            &mut cycle_stack,
        )?;
        pending.functions.insert(fn_name.clone(), lowered);
    }
    Ok(())
}
