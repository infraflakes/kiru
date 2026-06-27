use crate::compiler::error::{CompileError, SpannedValidationError};
use crate::compiler::merge::merge_project_body_stmt;
use crate::compiler::types::{Sanctuary, SyncMode};
use crate::dsl::{CasePattern, Expr, FnStmt};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn is_sanctuary_disabled() -> bool {
    std::env::var("SANCTUARY").as_deref() == Ok("0")
}

fn spanned_err(
    msg: String,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> CompileError {
    CompileError::ValidationReport(miette::Report::new(SpannedValidationError {
        message: msg,
        span: miette::SourceSpan::new(offset.into(), len.max(1)),
        source_code: miette::NamedSource::new(source_name, source_text.to_string()),
    }))
}

pub(crate) fn resolve_include(
    cfg: &mut Sanctuary,
    parse_recursive_fn: impl Fn(
        &Path,
        &mut HashSet<PathBuf>,
        &mut HashSet<PathBuf>,
    ) -> Result<Vec<crate::dsl::ast::Program>, CompileError>,
) -> Result<(), CompileError> {
    for proj in cfg.projects.values_mut() {
        let Some(include_file) = &proj.include_file else {
            continue;
        };

        if proj.sync == SyncMode::Ignore {
            continue;
        }

        let use_path = PathBuf::from(&cfg.sanctuary_path)
            .join(proj.dir.trim_start_matches('/'))
            .join(include_file.trim_start_matches('/'));

        if !use_path.exists() {
            return Err(spanned_err(
                format!(
                    "project {:?}: include file not found: {} (run 'kiru sync' first)",
                    proj.name,
                    use_path.display()
                ),
                "",
                "",
                0,
                1,
            ));
        }

        let mut loaded_files = HashSet::new();
        let mut recursion_stack = HashSet::new();
        let programs = parse_recursive_fn(&use_path, &mut loaded_files, &mut recursion_stack)?;

        let mut seen_fields: HashSet<String> = HashSet::new();
        for program in &programs {
            for stmt in &program.stmts {
                merge_project_body_stmt(
                    proj,
                    stmt.clone(),
                    &mut cfg.vars,
                    &program.source_name,
                    &program.source_text,
                    &mut seen_fields,
                )?;
            }
        }
    }

    Ok(())
}

pub fn validate(cfg: &Sanctuary) -> Result<(), CompileError> {
    let mut errs = Vec::new();

    if is_sanctuary_disabled() {
        // SANCTUARY=0 mode: sanctuary and project fields are optional
    } else if cfg.sanctuary_path.is_empty() {
        errs.push("sanctuary declaration is required".to_string());
    } else if !Path::new(&cfg.sanctuary_path).is_absolute() {
        errs.push(format!(
            "sanctuary path must be absolute: {}",
            cfg.sanctuary_path
        ));
    }

    if !is_sanctuary_disabled() {
        let mut dirs = HashSet::<String>::new();
        for proj in cfg.projects.values() {
            if proj.url.is_empty() {
                errs.push(format!("project {:?}: url is required", proj.name));
            }
            if proj.dir.is_empty() {
                errs.push(format!("project {:?}: dir is required", proj.name));
            }
            let normalized_dir = proj.dir.trim_start_matches('/').to_string();
            if !dirs.insert(normalized_dir) {
                errs.push(format!(
                    "project {:?}: duplicate directory {:?}",
                    proj.name, proj.dir
                ));
            }
        }
    }

    validate_run_refs(&cfg.runs, &cfg.functions, "top-level", &mut errs);

    let global_scope: HashSet<String> = cfg.vars.keys().cloned().collect();
    validate_fn_bodies(
        &cfg.functions,
        &global_scope,
        &HashSet::new(),
        "(top-level)",
        &mut errs,
    );

    for (proj_name, project) in &cfg.projects {
        validate_run_refs(&project.runs, &project.functions, proj_name, &mut errs);

        let project_scope: HashSet<String> = project.vars.keys().cloned().collect();
        validate_fn_bodies(
            &project.functions,
            &global_scope,
            &project_scope,
            proj_name,
            &mut errs,
        );
    }

    if !errs.is_empty() {
        return Err(spanned_err(errs.join("\n"), "", "", 0, 1));
    }

    Ok(())
}

fn validate_run_refs(
    runs: &std::collections::HashMap<String, Vec<Vec<String>>>,
    functions: &std::collections::HashMap<String, Vec<FnStmt>>,
    prefix: &str,
    errs: &mut Vec<String>,
) {
    for (run_name, chains) in runs {
        for chain in chains {
            for fn_name in chain {
                if !functions.contains_key(fn_name) {
                    errs.push(format!(
                        "{}: run {:?} references unknown function {:?}",
                        prefix, run_name, fn_name
                    ));
                }
            }
        }
    }
}

fn validate_fn_bodies(
    functions: &std::collections::HashMap<String, Vec<FnStmt>>,
    global_scope: &HashSet<String>,
    project_scope: &HashSet<String>,
    proj_name: &str,
    errs: &mut Vec<String>,
) {
    for (fn_name, body) in functions {
        let mut mutable_scope: HashSet<String> =
            global_scope.union(project_scope).cloned().collect();
        validate_fn_body(fn_name, body, &mut mutable_scope, errs, proj_name);
    }
}

fn validate_expr(
    expr: &Expr,
    fn_name: &str,
    scope: &HashSet<String>,
    errs: &mut Vec<String>,
    proj_name: &str,
) {
    match expr {
        Expr::VarRef { name, .. } => {
            if !scope.contains(name) {
                errs.push(format!(
                    "project {:?}: fn {:?}: undefined variable ${}",
                    proj_name, fn_name, name
                ));
            }
        }
        Expr::BacktickLit { parts, .. } => {
            for part in parts {
                if part.is_var {
                    let var_name = part.value.trim_start_matches('$');
                    if !scope.contains(var_name) {
                        errs.push(format!(
                            "project {:?}: fn {:?}: undefined variable ${}",
                            proj_name, fn_name, var_name
                        ));
                    }
                }
            }
        }
    }
}

fn validate_fn_body(
    fn_name: &str,
    body: &[FnStmt],
    scope: &mut HashSet<String>,
    errs: &mut Vec<String>,
    proj_name: &str,
) {
    for stmt in body {
        match stmt {
            FnStmt::VarDecl { name, value, .. } => {
                validate_expr(value, fn_name, scope, errs, proj_name);
                scope.insert(name.clone());
            }
            FnStmt::Log { value, .. } => validate_expr(value, fn_name, scope, errs, proj_name),
            FnStmt::Exec { value, .. } => validate_expr(value, fn_name, scope, errs, proj_name),
            FnStmt::Cd { value, .. } => validate_expr(value, fn_name, scope, errs, proj_name),
            FnStmt::EnvBlock { pairs, body, .. } => {
                let mut block_scope = scope.clone();
                for pair in pairs {
                    validate_expr(&pair.value, fn_name, scope, errs, proj_name);
                }
                validate_fn_body(fn_name, body, &mut block_scope, errs, proj_name);
            }
            FnStmt::Case { condition, scopes } => {
                validate_expr(condition, fn_name, scope, errs, proj_name);
                for arm in scopes {
                    match &arm.pattern {
                        CasePattern::VarRef { name } => {
                            validate_expr(
                                &Expr::VarRef {
                                    name: name.clone(),
                                    offset: 0,
                                    len: 0,
                                },
                                fn_name,
                                scope,
                                errs,
                                proj_name,
                            );
                        }
                        CasePattern::Literal { parts } => {
                            for part in parts {
                                if part.is_var && !scope.contains(&part.value) {
                                    errs.push(format!(
                                        "project {:?}: fn {:?}: undefined variable ${}",
                                        proj_name, fn_name, part.value
                                    ));
                                }
                            }
                        }
                        CasePattern::Default => {}
                    }
                    let mut arm_scope = scope.clone();
                    validate_fn_body(fn_name, &arm.body, &mut arm_scope, errs, proj_name);
                }
            }
        }
    }
}
