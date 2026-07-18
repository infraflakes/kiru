use crate::compiler::error::CompileError;
use crate::compiler::fnstmt::{ValidateFnCtx, validate_fn_body_stmts};
use crate::compiler::scope::{ScopeKind, ScopeStack};
use crate::dsl::{Expr, FnStmt};
use crate::error::spanned_report_on;
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
            &cfg.source_texts,
            &mut errors,
        );
    }

    if errors.is_empty() {
        Ok(())
    } else {
        // Return the original child diagnostics intact so each keeps its own
        // source name, labels, and spans when rendered.  Previously this
        // branch stringified every report and wrapped them in a fresh
        // `miette!` report, discarding their spans.
        Err(CompileError::ValidationReport(errors))
    }
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
/// Function frame per function, then dispatches each statement to its own
/// `validate` via the shared [`ValidateFnCtx`].
fn validate_project_bodies(
    functions: &HashMap<String, Vec<FnStmt>>,
    global: &ScopeStack<String>,
    declared_var_names: &HashSet<String>,
    proj_name: &str,
    sources: &HashMap<String, String>,
    errors: &mut Vec<miette::Report>,
) {
    for (fn_name, body) in functions {
        let mut scope = ScopeStack::<()>::new();
        scope.seed_global(global.iter_global().map(|(k, _)| (k.clone(), ())));
        scope.push_frame(ScopeKind::Project);
        scope.seed_top(declared_var_names.iter().map(|k| (k.clone(), ())));

        let guard = scope.enter(ScopeKind::Function);
        let mut ctx = ValidateFnCtx {
            fn_name,
            proj_name,
            scope: &mut *guard.stack,
            errors: &mut *errors,
            sources,
        };
        validate_fn_body_stmts(body, &mut ctx);
    }
}

/// Check that all variable references in an `Expr` are defined in the
/// current scope hierarchy. Undefined references become a spanned diagnostic
/// pointing at the exact expression, so the error reports the location like
/// every other syntax/validation error.
pub(crate) fn validate_expr(
    expr: &Expr,
    fn_name: &str,
    scope: &ScopeStack<()>,
    errors: &mut Vec<miette::Report>,
    proj_name: &str,
    sources: &HashMap<String, String>,
) {
    expr.visit_vars(|name| {
        if !scope.is_declared(name) {
            errors.push(spanned_report_on(
                format!(
                    "project {:?}: fn {:?}: undefined variable ${}",
                    proj_name, fn_name, name
                ),
                sources,
                expr,
            ));
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::compiler::test_support::*;

    #[test]
    fn test_undefined_variable() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var string x = $missing;\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_undefined_var_in_fn_body() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn badfn { log $undefined; }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_run_reference_validation() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn real { log `hi`; }\n\
            run s { unknown; }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("unknown function"), "got: {}", err_str);
    }

    #[test]
    fn test_valid_run_references() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn real { log `hi`; }\n\
            run s { real; }\n\
        }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.projects["test"].runs.contains_key("s"));
    }

    #[test]
    fn test_undefined_var_in_case_condition() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn badfn { case $undefined { _ { }; }; }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_undefined_var_in_case_varref_pattern() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn badfn { var string x = `ok`; case $x { $undefined { }; _ { }; }; }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_run_validates_function_refs() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr p [ url = `http://x` dir = `x` ] {\n\
            run bad { nonexistent; }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("unknown function"), "got: {}", err);
    }

    #[test]
    fn test_fn_var_validation() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr p [ url = `http://x` dir = `x` ] {\n\
            fn bad { log $undefined; }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_validation_errors_span_multiple_source_files() {
        // Two undefined-variable validation errors that originate from DIFFERENT
        // source files (main.kiru and an imported build.kiru). The aggregate
        // must preserve each child report's own source/span, so both surface
        // with their correct file instead of being collapsed into one
        // stringified blob.
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
pr p [ url = `u` dir = `d` ] {\n\
    fn f1 { log $missing_main; }\n\
}\n\
import `build.kiru`;\n\
            ",
        );
        write_config(
            dir.path(),
            "build.kiru",
            "\
pr p {\n\
    fn f2 { log $missing_build; }\n\
}\n\
            ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("undefined variable $missing_main")
                && err_str.contains("undefined variable $missing_build"),
            "both source-file errors should be preserved in the aggregate, got: {}",
            err_str
        );
        assert!(
            !err_str.contains("validation error(s) found"),
            "aggregate must keep original diagnostics, not stringify-and-wrap, got: {}",
            err_str
        );
    }
}
