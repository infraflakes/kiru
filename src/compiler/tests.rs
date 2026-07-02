use super::*;
use crate::runner::Runner;
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn compile_full(entry_path: &Path) -> Result<Config, CompileError> {
    compile_and_resolve(entry_path)
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
var string a = `hello`;\n\
pr test [url = `http://example.com` dir = `test`] { }\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
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
pr test [\n\
    url = `http://example.com`\n\
    dir = `test`\n\
] {\n\
    var string app = `todo`;\n\
    fn build { log `hi`; }\n\
    run release { build; }\n\
    run ci { build; }\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
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
import `./other.kiru`;\n\
pr p [url = $extra dir = `d`] { }
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.projects["p"].url, "from-other");
}

#[test]
fn test_circular_import() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(dir.path(), "a.kiru", "import `./b.kiru`;");
    write_config(dir.path(), "b.kiru", "import `./a.kiru`;");
    let err = compile_full(&dir.path().join("a.kiru")).unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("circular") || err_str.contains("Circular"),
        "got: {}",
        err_str
    );
}

#[test]
fn test_shadowing_global_var() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
var string x = `a`;\n\
var string x = `b`;\n\
pr p [url = $x dir = `d`] { }
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    // Later declaration shadows earlier one (top-down evaluation)
    assert_eq!(cfg.projects["p"].url, "b");
}

#[test]
fn test_duplicate_project_merges() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p1 [url = `u` dir = `d1`] { }\n\
pr p1 { fn build { log `x`; } }\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert!(cfg.projects.contains_key("p1"));
    let proj = &cfg.projects["p1"];
    assert_eq!(proj.url, "u");
    assert_eq!(proj.dir, "d1");
    assert!(proj.functions.contains_key("build"));
}

#[test]
fn test_variable_chain_resolution() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
var string a = `x`;\n\
var string b = $a;\n\
var string c = $b;\n\
pr p [url = $c dir = `d`] { }
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.projects["p"].url, "x");
}

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
fn test_missing_url() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p [dir = `d`] { }\
",
    );
    compile_full(&dir.path().join("main.kiru")).unwrap();
}

#[test]
fn test_missing_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p [url = `u`] { }\
",
    );
    compile_full(&dir.path().join("main.kiru")).unwrap();
}

#[test]
fn test_duplicate_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr a [url = `ua` dir = `shared`] { }\n\
pr b [url = `ub` dir = `shared`] { }\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
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
pr p [url = `u` dir = `d` sync = `invalid`] { }\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(err.to_string().contains("sync"), "got: {}", err);
}

#[test]
fn test_duplicate_project_field() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p [url = `u` dir = `d` dir = `e`] { }\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(err.to_string().contains("duplicate"), "got: {}", err);
}

#[test]
fn test_interpolation_in_backtick() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
var string name = `world`;\n\
pr p [url = `http://${name}.com` dir = `d`] { }\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.projects["p"].url, "http://world.com");
}

#[test]
fn test_project_field_with_var_ref() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
var string myurl = `http://example.com`;\n\
pr x [url = $myurl dir = `d`] { }\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.projects["x"].url, "http://example.com");
}

#[test]
fn test_duplicate_fn_in_project() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    fn dup { log `a`; }\n\
    fn dup { log `b`; }\n\
}\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
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
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    fn check { log `x`; }\n\
    run dup { check; }\n\
    run dup { check; }\n\
}\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
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
import `./a.kiru`;\n\
pr p [url = $a dir = `d`] { }\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.projects["p"].url, "from-a");
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
fn test_shadowing_var_in_fn_body() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    fn bad {\n\
        var string x = `a`;\n\
        var string x = `b`;\n\
    }\n\
}\
",
    );
    // Shadowing is allowed in fn bodies — latest declaration wins within its scope
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert!(cfg.projects["test"].functions.contains_key("bad"));
    // VarDecls are inlined at compile time, so the resolved body is empty
    let body = &cfg.projects["test"].functions["bad"];
    assert_eq!(body.len(), 0);
}

#[test]
fn test_project_var_chain_resolution() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    var string a = `hello`;\n\
    var string b = $a;\n\
}\
",
    );
    // We can't check project vars directly on the resolved Config,
    // but the configuration should compile and resolve without errors.
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.projects["test"].dir, "d");
}

#[test]
fn test_project_var_sees_global() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
var string global_var = `global`;\n\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    fn f { log $global_var; }\n\
}\
",
    );
    // global vars should be accessible inside project function bodies
    compile_full(&dir.path().join("main.kiru")).unwrap();
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
fn test_case_runtime_matching_arm() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `http://example.com`\n\
    dir = `test`\n\
] {\n\
    var string os = `Linux`;\n\
    fn deploy {\n\
        case $os {\n\
            `Linux` { log `matched`; };\n\
            _ { log `default`; };\n\
        };\n\
    }\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let mut runner = Runner::new(Arc::new(cfg));
    runner.execute_fn_call("deploy", "test").unwrap();
}

#[test]
fn test_case_runtime_no_match() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `http://example.com`\n\
    dir = `test`\n\
] {\n\
    var string os = `Darwin`;\n\
    fn deploy {\n\
        case $os {\n\
            `Linux` { log `only-linux`; };\n\
        };\n\
    }\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let mut runner = Runner::new(Arc::new(cfg));
    runner.execute_fn_call("deploy", "test").unwrap();
}

// --- Project-scoped fn/run collection ---

#[test]
fn test_project_fn_collection() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p [ url = `http://x` dir = `x` ] {\n\
    fn build { log `building`; }\n\
    fn test { exec `check`; }\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["p"];
    assert!(proj.functions.contains_key("build"));
    assert!(proj.functions.contains_key("test"));
    assert_eq!(proj.functions.len(), 2);
}

#[test]
fn test_project_run_collection() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p [ url = `http://x` dir = `x` ] {\n\
    fn build { log `x`; }\n\
    fn test { log `y`; }\n\
    run all { build => test; }\n\
    run ci { build; }\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["p"];
    assert!(proj.runs.contains_key("all"));
    assert!(proj.runs.contains_key("ci"));
    assert_eq!(proj.runs.len(), 2);
    assert_eq!(proj.runs["all"], vec![vec!["build", "test"]]);
}

#[test]
fn test_duplicate_fn_in_project_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p [ url = `http://x` dir = `x` ] {\n\
    fn dup { log `a`; }\n\
    fn dup { log `b`; }\n\
}\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(
        err.to_string().contains("duplicate function"),
        "got: {}",
        err
    );
}

#[test]
fn test_duplicate_run_in_project_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p [ url = `http://x` dir = `x` ] {\n\
    fn x { log `a`; }\n\
    run dup { x; }\n\
    run dup { x; }\n\
}\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(err.to_string().contains("duplicate run"), "got: {}", err);
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

// --- Shadowing and field/var interleaving (new model) ---

#[test]
fn test_project_field_references_project_var() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `http://example.com/${name}`\n\
    dir = $name\n\
] {\n\
    var string name = `myproject`;\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
    assert_eq!(proj.url, "http://example.com/myproject");
    assert_eq!(proj.dir, "myproject");
}

#[test]
fn test_global_var_shadowed_by_project_var() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
var string name = `global`;\n\
pr test [\n\
    url = `http://example.com`\n\
    dir = $name\n\
] {\n\
    var string name = `project`;\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
    // Project-level var "name" shadows the global "name"
    assert_eq!(proj.dir, "project");
}
