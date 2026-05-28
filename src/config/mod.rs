pub(crate) mod error;
pub(crate) mod merge;
pub(crate) mod types;
pub(crate) mod validation;

pub use error::ConfigError;
pub use types::{Config, Project};

use crate::dsl::ast::{Expr, Program, Stmt};
use crate::dsl::lexer::Lexer;
use crate::dsl::parser::Parser;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn load(entry_path: &Path) -> Result<Config, ConfigError> {
    let abs_path = if entry_path.is_absolute() {
        entry_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(ConfigError::Io)?
            .join(entry_path)
    };

    let mut loaded_files = HashSet::new();
    let mut recursion_stack = HashSet::new();
    let programs = parse_recursive(&abs_path, &mut loaded_files, &mut recursion_stack)?;

    let config = merge::merge(programs)?;
    validation::validate_base(&config)?;

    Ok(config)
}

pub fn resolve_uses(cfg: &mut Config) -> Result<(), ConfigError> {
    validation::resolve_use(cfg, parse_recursive)
}

fn parse_recursive(
    file_path: &Path,
    loaded_files: &mut HashSet<PathBuf>,
    recursion_stack: &mut HashSet<PathBuf>,
) -> Result<Vec<Program>, ConfigError> {
    let abs_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(ConfigError::Io)?
            .join(file_path)
    };

    let canon_path = std::fs::canonicalize(&abs_path).map_err(|e| {
        ConfigError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to resolve {}: {}", abs_path.display(), e),
        ))
    })?;

    if recursion_stack.contains(&canon_path) {
        return Err(ConfigError::CircularImport(
            canon_path.display().to_string(),
        ));
    }

    if loaded_files.contains(&canon_path) {
        return Ok(Vec::new());
    }

    recursion_stack.insert(canon_path.clone());

    let data = std::fs::read_to_string(&canon_path).map_err(|e| {
        recursion_stack.remove(&canon_path);
        ConfigError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to read {}: {}", canon_path.display(), e),
        ))
    })?;

    let source_name = canon_path.display().to_string();
    let lexer = Lexer::new(data);
    let mut parser = Parser::new(lexer);
    let program = match parser.parse() {
        Ok(prog) => prog,
        Err(errors) => {
            recursion_stack.remove(&canon_path);
            let source = parser.into_source();
            let reports: Vec<miette::Report> = errors
                .into_iter()
                .map(|error| {
                    miette::Report::new(error).with_source_code(miette::NamedSource::new(
                        source_name.clone(),
                        source.clone(),
                    ))
                })
                .collect();
            return Err(ConfigError::ParseReports(reports));
        }
    };

    let mut results = Vec::new();

    let base_dir = canon_path.parent().unwrap_or_else(|| Path::new("."));

    for stmt in &program.stmts {
        if let Stmt::ImportDecl { path } = stmt {
            let rel_path = match path {
                Expr::BacktickLit { parts } => {
                    let mut s = String::new();
                    for part in parts {
                        if part.is_var {
                            return Err(ConfigError::Validation(format!(
                                "variable interpolation in import path is not supported: ${{{}}}",
                                part.value
                            )));
                        }
                        s.push_str(&part.value);
                    }
                    s
                }
                Expr::VarRef { name } => {
                    return Err(ConfigError::Validation(format!(
                        "variable reference in import path is not supported: ${}",
                        name
                    )));
                }
            };
            let import_abs = base_dir.join(&rel_path);
            match parse_recursive(&import_abs, loaded_files, recursion_stack) {
                Ok(imported) => results.extend(imported),
                Err(e) => {
                    recursion_stack.remove(&canon_path);
                    return Err(e);
                }
            }
        }
    }

    recursion_stack.remove(&canon_path);
    loaded_files.insert(canon_path);
    results.push(program);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::Runner;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    fn load_full(entry_path: &Path) -> Result<Config, ConfigError> {
        let mut cfg = load(entry_path)?;
        resolve_uses(&mut cfg)?;
        Ok(cfg)
    }

    fn write_config(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        fs::write(&path, content)
            .unwrap_or_else(|e| panic!("failed to write {}: {}", path.display(), e));
    }

    #[test]
    fn test_load_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp/dev`;\n\
var string a = `hello`;\n\
pr test { url = `http://example.com`; dir = `test`; }\n\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.shell, "bash");
        assert_eq!(cfg.sanctuary, "/tmp/dev");
        assert_eq!(cfg.vars.get("a").unwrap(), "hello");
        assert!(cfg.projects.contains_key("test"));
        assert_eq!(cfg.projects["test"].url, "http://example.com");
    }

    #[test]
    fn test_load_with_project_body() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp/dev`;\n\
pr test {\n\
    url = `http://example.com`;\n\
    dir = `test`;\n\
    var string app = `todo`;\n\
    fn build { log `hi`; }\n\
    run release { build; }\n\
    run ci { build; }\n\
}\n\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert_eq!(proj.vars.get("app").unwrap(), "todo");
        assert!(proj.functions.contains_key("build"));
        assert!(proj.runs.contains_key("release"));
        assert!(proj.runs.contains_key("ci"));
        assert_eq!(proj.runs["release"], vec![vec!["build"]]);
        assert_eq!(proj.runs["ci"], vec![vec!["build"]]);
    }

    #[test]
    fn test_import_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(dir.path(), "other.kiru", "var string extra = `from-other`;");
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
import `./other.kiru`;\n\
var string x = $extra;\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.vars.get("x").unwrap(), "from-other");
    }

    #[test]
    fn test_circular_import() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "a.kiru",
            "shell = `bash`; import `./b.kiru`; sanctuary = `/tmp`;",
        );
        write_config(
            dir.path(),
            "b.kiru",
            "shell = `bash`; import `./a.kiru`; sanctuary = `/tmp`;",
        );
        let err = load(&dir.path().join("a.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("circular") || err_str.contains("Circular"),
            "got: {}",
            err_str
        );
    }

    #[test]
    fn test_duplicate_sanctuary() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
sanctuary = `/other`;\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate sanctuary"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_variable_decl() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
var string x = `a`;\n\
var string x = `b`;\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_project() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr p1 { url = `u`; dir = `d1`; }\n\
pr p1 { url = `u2`; dir = `d2`; }\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate project"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_variable_chain_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
var string a = `x`;\n\
var string b = $a;\n\
var string c = $b;\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.vars["a"], "x");
        assert_eq!(cfg.vars["b"], "x");
        assert_eq!(cfg.vars["c"], "x");
    }

    #[test]
    fn test_undefined_variable() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
var string x = $missing;\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_missing_shell() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
sanctuary = `/tmp`;\n\
pr test { url = `http://example.com`; dir = `test`; }\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("shell"), "got: {}", err);
    }

    #[test]
    fn test_missing_sanctuary() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
pr test { url = `http://example.com`; dir = `test`; }\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("sanctuary"), "got: {}", err);
    }

    #[test]
    fn test_sanctuary_absolute_path() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `relative/path`;\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("absolute"), "got: {}", err);
    }

    #[test]
    fn test_missing_url() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr p { dir = `d`; }\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("url is required"), "got: {}", err);
    }

    #[test]
    fn test_missing_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr p { url = `u`; }\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("dir is required"), "got: {}", err);
    }

    #[test]
    fn test_duplicate_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr a { url = `ua`; dir = `shared`; }\n\
pr b { url = `ub`; dir = `shared`; }\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate directory"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_invalid_sync_value() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr p { url = `u`; dir = `d`; sync = `invalid`; }\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("sync"), "got: {}", err);
    }

    #[test]
    fn test_empty_config() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(dir.path(), "main.kiru", "");
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("shell"), "got: {}", err);
    }

    #[test]
    fn test_only_shell_and_sanctuary() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.shell, "bash");
        assert_eq!(cfg.sanctuary, "/tmp");
    }

    #[test]
    fn test_interpolation_in_backtick() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
var string name = `world`;\n\
var string greeting = `hello ${name}`;\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.vars["greeting"], "hello world");
    }

    #[test]
    fn test_project_field_with_var_ref() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
var string myurl = `http://example.com`;\n\
pr x { url = $myurl; dir = `d`; }\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["x"].url, "http://example.com");
    }

    #[test]
    fn test_duplicate_fn_in_project() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn dup { log `a`; }\n\
    fn dup { log `b`; }\n\
}\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate function"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_run_in_project() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn check { log `x`; }\n\
    run dup { check; }\n\
    run dup { check; }\n\
}\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate run block"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_par_in_project() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn check { log `x`; }\n\
    run dup { check; }\n\
    run dup { check; }\n\
}\
",
        );
        let err = load(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate run block"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_multi_file_parse_order() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(dir.path(), "a.kiru", "var string a = `from-a`;");
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
import `./a.kiru`;\n\
var string b = $a;\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.vars["b"], "from-a");
    }

    #[test]
    fn test_undefined_var_in_fn_body() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn badfn { log $undefined; }\n\
}\
",
        );
        let err = load_full(&dir.path().join("main.kiru")).unwrap_err();
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
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn real { log `hi`; }\n\
    run s { unknown; }\n\
}\
",
        );
        let err = load_full(&dir.path().join("main.kiru")).unwrap_err();
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
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn real { log `hi`; }\n\
    run s { real; }\n\
}\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.projects["test"].runs.contains_key("s"));
    }

    #[test]
    fn test_duplicate_var_in_fn_body() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn bad {\n\
        var string x = `a`;\n\
        var string x = `b`;\n\
    }\n\
}\
",
        );
        let err = load_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_use_file_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        let proj_dir = dir.path().join("test");
        std::fs::create_dir_all(&proj_dir).unwrap();
        write_config(
            &proj_dir,
            "use.kiru",
            "\
var string usevar = `from-use`;\n\
fn usefn { log `from-use`; }\n\
run useseq { usefn; }\n\
run usepar { usefn; }\
",
        );
        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
shell = `bash`;\n\
sanctuary = `{}`;\n\
pr test {{ url = `http://example.com`; dir = `test`; use = `use.kiru`; }}\
",
                dir.path().display()
            ),
        );
        let cfg = load_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert_eq!(proj.vars.get("usevar").unwrap(), "from-use");
        assert!(proj.functions.contains_key("usefn"));
        assert!(proj.runs.contains_key("useseq"));
        assert!(proj.runs.contains_key("usepar"));
    }

    #[test]
    fn test_use_file_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
shell = `bash`;\n\
sanctuary = `{}`;\n\
pr test {{ url = `http://example.com`; dir = `test`; use = `nonexistent.kiru`; }}\
",
                dir.path().display()
            ),
        );
        let err = load_full(&dir.path().join("main.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("use file not found") || err_str.contains("not found"),
            "got: {}",
            err_str
        );
    }

    #[test]
    fn test_use_file_sync_ignore_skips() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
shell = `bash`;\n\
sanctuary = `{}`;\n\
pr test {{ url = `http://example.com`; dir = `test`; sync = `ignore`; use = `use.kiru`; }}\
",
                dir.path().display()
            ),
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.projects.contains_key("test"));
    }

    #[test]
    fn test_shell_exec_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
var shell test_var = `echo hello`;\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.vars["test_var"], "hello");
    }

    #[test]
    fn test_sanctuary_with_var_ref() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
shell = `bash`;\n\
var shell workdir = `echo {}`;\n\
sanctuary = $workdir;\
",
                dir.path().display()
            ),
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.sanctuary, dir.path().to_str().unwrap());
    }

    #[test]
    fn test_project_var_chain_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    var string a = `hello`;\n\
    var string b = $a;\n\
}\
",
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert_eq!(proj.vars["a"], "hello");
        assert_eq!(proj.vars["b"], "hello");
    }

    #[test]
    fn test_project_var_sees_global() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
var string global_var = `global`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn f { log $global_var; }\n\
}\
",
        );
        // global vars should be accessible inside project function bodies
        load_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_undefined_var_in_case_condition() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn badfn { case $undefined { _ { }; }; }\n\
}\
",
        );
        let err = load_full(&dir.path().join("main.kiru")).unwrap_err();
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
shell = `bash`;\n\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    fn badfn { var string x = `ok`; case $x { $undefined { }; _ { }; }; }\n\
}\
",
        );
        let err = load_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_case_runtime_matching_arm() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
shell = `bash`;\n\
sanctuary = `{}`;\n\
pr test {{\n\
    url = `http://example.com`;\n\
    dir = `test`;\n\
    var string os = `Linux`;\n\
    fn deploy {{\n\
        case $os {{\n\
            `Linux` {{ log `matched`; }};\n\
            _ {{ log `default`; }};\n\
        }};\n\
    }}\n\
}}\
",
                dir.path().display()
            ),
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        let mut runner = Runner::from_arc(Arc::new(cfg));
        runner.execute_fn_call("deploy", "test").unwrap();
    }

    #[test]
    fn test_case_runtime_no_match() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
shell = `bash`;\n\
sanctuary = `{}`;\n\
pr test {{\n\
    url = `http://example.com`;\n\
    dir = `test`;\n\
    var string os = `Darwin`;\n\
    fn deploy {{\n\
        case $os {{\n\
            `Linux` {{ log `only-linux`; }};\n\
        }};\n\
    }}\n\
}}\
",
                dir.path().display()
            ),
        );
        let cfg = load(&dir.path().join("main.kiru")).unwrap();
        let mut runner = Runner::from_arc(Arc::new(cfg));
        // No matching arm — silently does nothing, no error.
        runner.execute_fn_call("deploy", "test").unwrap();
    }
}
