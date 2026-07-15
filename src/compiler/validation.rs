use crate::compiler::error::CompileError;
use crate::compiler::error::SpannedValidationError;
use crate::compiler::resolve;
use crate::compiler::scope::{ScopeKind, ScopeStack};
use crate::dsl::{Expr, FnStmt};
use miette::miette;
use std::collections::{HashMap, HashSet};

/// Validate an `UnresolvedConfig` against the global var scope,
/// collecting all errors before returning.
pub fn validate_configuration(
    cfg: &super::types::UnresolvedConfig,
    global: &ScopeStack<String>,
) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    for (proj_name, project) in &cfg.projects {
        validate_run_refs(&project.runs, &project.functions, proj_name, &mut errors);

        validate_project_bodies(
            &project.functions,
            global,
            &project.declared_var_names,
            proj_name,
            &project.source_file,
            &project.source_text,
            &mut errors,
        );
    }

    if errors.len() == 1 {
        return Err(CompileError::ValidationReport(
            errors.into_iter().next().unwrap(),
        ));
    } else if !errors.is_empty() {
        let mut combined = String::new();
        for (i, report) in errors.iter().enumerate() {
            if i > 0 {
                combined.push('\n');
            }
            combined.push_str(&format!("{}", report));
        }
        return Err(CompileError::ValidationReport(miette!(
            "{}\n{} validation error(s) found",
            combined,
            errors.len()
        )));
    }

    Ok(())
}

/// Check that all run chains reference functions that exist in the
/// project's function map.
fn validate_run_refs(
    runs: &HashMap<String, Vec<Vec<String>>>,
    functions: &HashMap<String, Vec<FnStmt>>,
    prefix: &str,
    errors: &mut Vec<miette::Report>,
) {
    for (run_name, chains) in runs {
        for chain in chains {
            for fn_name in chain {
                if !functions.contains_key(fn_name) {
                    errors.push(miette!(
                        "{}: run {:?} references unknown function {:?}",
                        prefix,
                        run_name,
                        fn_name
                    ));
                }
            }
        }
    }
}

/// Validate all function bodies in a project's function map.  Builds a
/// scope stack seeded with global + project vars and pushes a fresh
/// Function frame per function.
fn validate_project_bodies(
    functions: &HashMap<String, Vec<FnStmt>>,
    global: &ScopeStack<String>,
    declared_var_names: &HashSet<String>,
    proj_name: &str,
    source_name: &str,
    source_text: &str,
    errors: &mut Vec<miette::Report>,
) {
    for (fn_name, body) in functions {
        let mut scope = ScopeStack::<()>::new();
        scope.seed_global(global.iter_global().map(|(k, _)| (k.clone(), ())));
        scope.push_frame(ScopeKind::Project);
        scope.seed_top(declared_var_names.iter().map(|k| (k.clone(), ())));

        let guard = scope.enter(ScopeKind::Function);
        validate_fn_body(
            fn_name,
            body,
            &mut *guard.stack,
            errors,
            proj_name,
            source_name,
            source_text,
        );
    }
}

/// Check that all variable references in an `Expr` are defined in the
/// current scope hierarchy.
fn validate_expr(
    expr: &Expr,
    fn_name: &str,
    scope: &ScopeStack<()>,
    errors: &mut Vec<miette::Report>,
    proj_name: &str,
) {
    resolve::visit_expr_vars(expr, |name| {
        if !scope.is_declared(name) {
            errors.push(miette!(
                "project {:?}: fn {:?}: undefined variable ${}",
                proj_name,
                fn_name,
                name
            ));
        }
    });
}

/// Validate variable references and duplicate declarations in a function
/// body.  The caller owns the scope; this function does not push/pop
/// frames at the top level but handles case-arm frames internally.
fn validate_fn_body(
    fn_name: &str,
    body: &[FnStmt],
    scope: &mut ScopeStack<()>,
    errors: &mut Vec<miette::Report>,
    proj_name: &str,
    source_name: &str,
    source_text: &str,
) {
    for stmt in body {
        match stmt {
            FnStmt::VarDecl { name, value, .. } => {
                validate_expr(value, fn_name, scope, errors, proj_name);
                if scope.is_declared(name) {
                    let (offset, len) = value.offset_len();
                    let kind = scope.declaring_kind(name).unwrap_or(ScopeKind::Global);
                    errors.push(miette::Report::new(SpannedValidationError {
                        message: format!("${} is already defined at {}", name, kind),
                        span: miette::SourceSpan::new(offset.into(), len.max(1)),
                        source_code: miette::NamedSource::new(source_name, source_text.to_owned()),
                    }));
                }
                let _ = scope.declare(name.clone(), ());
            }
            FnStmt::Log { value, .. } => {
                validate_expr(value, fn_name, scope, errors, proj_name);
            }
            FnStmt::Exec { value, .. } => {
                validate_expr(value, fn_name, scope, errors, proj_name);
            }
            FnStmt::Cd { value, .. } => {
                validate_expr(value, fn_name, scope, errors, proj_name);
            }
            FnStmt::EnvBlock { pairs, body, .. } => {
                for pair in pairs {
                    validate_expr(&pair.value, fn_name, scope, errors, proj_name);
                }
                validate_fn_body(
                    fn_name,
                    body,
                    scope,
                    errors,
                    proj_name,
                    source_name,
                    source_text,
                );
            }
            FnStmt::Case { condition, scopes } => {
                validate_expr(condition, fn_name, scope, errors, proj_name);
                for arm in scopes {
                    resolve::visit_case_pattern_vars(&arm.pattern, |name| {
                        if !scope.is_declared(name) {
                            errors.push(miette!(
                                "project {:?}: fn {:?}: undefined variable ${}",
                                proj_name,
                                fn_name,
                                name
                            ));
                        }
                    });
                    let guard = scope.enter(ScopeKind::Case);
                    validate_fn_body(
                        fn_name,
                        &arm.body,
                        &mut *guard.stack,
                        errors,
                        proj_name,
                        source_name,
                        source_text,
                    );
                }
            }
        }
    }
}
