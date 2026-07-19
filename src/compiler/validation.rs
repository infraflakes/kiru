use crate::compiler::error::CompileError;
use crate::compiler::fnstmt::{ValidateFnCtx, validate_fn_body_stmts};
use crate::compiler::namespaces::Namespaces;
use crate::compiler::types::{UnresolvedConfig, UnresolvedProject};
use crate::dsl::FnStmt;
use crate::dsl::ast::QualifiedFnRef;
use std::collections::HashMap;

/// Whether `ns::name` resolves to a variable in the namespaces map: a global
/// variable or a project variable. A project's `url`/`dir`/`sync`/`branch`
/// metadata fields are never referenceable, so they are not considered here.
pub(crate) fn is_var_defined(namespaces: &Namespaces, ns: &str, name: &str) -> bool {
    if ns == "global" {
        return namespaces.global.contains_key(name);
    }
    match namespaces.projects.get(ns) {
        Some(p) => p.vars.contains_key(name),
        None => false,
    }
}

/// Validate an [`UnresolvedConfig`] against the namespaces map built by the
/// declare pass, collecting all errors before returning.
pub fn validate_configuration(
    cfg: &UnresolvedConfig,
    namespaces: &Namespaces,
    sources: &HashMap<String, String>,
) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    // Validate project function bodies and any project-scoped data.
    for (proj_name, project) in &cfg.projects {
        validate_project_bodies(
            &project.functions,
            proj_name,
            namespaces,
            sources,
            &mut errors,
        );
    }

    // Validate global run blocks.
    validate_run_refs(&cfg.runs, "<global>", &cfg.projects, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        // Return the original child diagnostics intact so each keeps its own
        // source name, labels, and spans when rendered.
        Err(CompileError::ValidationReport(errors))
    }
}

/// Check that all run chains reference functions that exist. A reference is
/// always `project::function`; the named project must exist and declare the
/// function.
fn validate_run_refs(
    runs: &HashMap<String, Vec<Vec<QualifiedFnRef>>>,
    proj_name: &str,
    projects: &HashMap<String, UnresolvedProject>,
    errors: &mut Vec<miette::Report>,
) {
    for (run_name, chains) in runs {
        for chain in chains {
            for q in chain {
                match projects.get(&q.project) {
                    Some(proj) => {
                        if !proj.functions.contains_key(&q.function) {
                            errors.push(miette::miette!(
                                "{}: run {:?} references unknown function {:?} in project {:?}",
                                proj_name,
                                run_name,
                                q.function,
                                q.project
                            ));
                        }
                    }
                    None => errors.push(miette::miette!(
                        "{}: run {:?} references unknown project {:?}",
                        proj_name,
                        run_name,
                        q.project
                    )),
                }
            }
        }
    }
}

/// Validate all function bodies in a project's function map. Every declared
/// variable already lives in `namespaces` (populated by the declare pass), so
/// reference checks are a single lookup; per-fn bodies are validated in turn.
fn validate_project_bodies(
    functions: &HashMap<String, Vec<FnStmt>>,
    proj_name: &str,
    namespaces: &Namespaces,
    sources: &HashMap<String, String>,
    errors: &mut Vec<miette::Report>,
) {
    for (fn_name, body) in functions {
        let mut ctx = ValidateFnCtx {
            fn_name,
            proj_name,
            namespaces,
            errors: &mut *errors,
            sources,
        };
        validate_fn_body_stmts(body, &mut ctx);
    }
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
         var string x = $global::missing;\
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
             fn badfn { log $test::undefined; }\n\
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
          }\n\
          run s { test::unknown; }\
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
          }\n\
          run s { test::real; }\
          ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.runs.contains_key("s"));
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
             fn badfn { case $test::undefined { _ { }; }; }\n\
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
             fn badfn { var string x = `ok`; case $test::x { $test::undefined { }; _ { }; }; }\n\
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
         }\n\
         run bad { p::nonexistent; }\
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
             fn bad { log $p::undefined; }\n\
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
 pr p [ url = `u` dir = `d` ] {
     fn f1 { log $p::missing_main; }
 }
 import `build.kiru`;
            ",
        );
        write_config(
            dir.path(),
            "build.kiru",
            "\
 pr p {
     fn f2 { log $p::missing_build; }
 }
            ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("undefined variable")
                && err_str.contains("p::missing_main")
                && err_str.contains("p::missing_build"),
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
