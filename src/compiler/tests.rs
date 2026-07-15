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

/// RAII guard that overrides `KIRU_CWD` for the duration of a test and
/// restores the previous value on drop.  Prevents test interference when
/// the parent process (e.g. `kiru` itself) has the env var set.
struct KiruCwdGuard(Option<bool>);
impl KiruCwdGuard {
    /// Opt out of `KIRU_CWD` — project-scope `var shell` tests expect
    /// the project working directory, not the current process directory.
    fn with_project_dir() -> Self {
        KiruCwdGuard(resolve::__test_set_kiru_cwd(Some(false)))
    }
    /// Opt into `KIRU_CWD` — verify the env-var override forces CWD.
    fn with_kiru_cwd() -> Self {
        KiruCwdGuard(resolve::__test_set_kiru_cwd(Some(true)))
    }
}
impl Drop for KiruCwdGuard {
    fn drop(&mut self) {
        resolve::__test_set_kiru_cwd(self.0);
    }
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
fn test_duplicate_global_var() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
var string x = `a`;\n\
var string x = `b`;\n\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(
        err.to_string().contains("$x is already defined"),
        "got: {}",
        err
    );
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
    assert_eq!(proj.dir, dir.path().join("d1").to_string_lossy());
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
fn test_kiru_cwd_forces_current_dir_for_project_scope_var_shell() {
    let dir = tempfile::TempDir::new().unwrap();
    let subdir = dir.path().join("projectdir");
    std::fs::create_dir(&subdir).unwrap();
    let current_dir = std::env::current_dir().unwrap();

    write_config(
        dir.path(),
        "main.kiru",
        &format!(
            "\
pr test [\n\
    url = `http://example.com`\n\
    dir = `{}`\n\
] {{\n\
    var shell cwd = `pwd`;\n\
    fn check {{\n\
        log $cwd;\n\
    }}\n\
}}\n\
",
            subdir.to_string_lossy()
        ),
    );

    let _guard = KiruCwdGuard::with_kiru_cwd();
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();

    let proj = &cfg.projects["test"];
    let fn_body = &proj.functions["check"];
    assert_eq!(fn_body.len(), 1);
    match &fn_body[0] {
        ResolvedFnStmt::Log { value } => {
            let expected = current_dir.to_string_lossy().to_string();
            assert_eq!(*value, expected);
        }
        other => panic!("expected Log, got {:?}", other),
    }
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
fn test_duplicate_var_in_fn_body() {
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
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(
        err.to_string().contains("$x is already defined"),
        "got: {}",
        err
    );
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
    assert_eq!(
        cfg.projects["test"].dir,
        dir.path().join("d").to_string_lossy()
    );
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
    assert_eq!(proj.dir, dir.path().join("myproject").to_string_lossy());
}

#[test]
fn test_project_var_cannot_shadow_global() {
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
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(
        err.to_string()
            .contains("$name is already defined at top level"),
        "got: {}",
        err
    );
}

// --- No-shadowing false-positive fixes ---

#[test]
fn test_sibling_fns_same_var_name_no_error() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    fn a { var string x = `a`; log $x; }\n\
    fn b { var string x = `b`; log $x; }\n\
}\
",
    );
    compile_full(&dir.path().join("main.kiru")).unwrap();
}

#[test]
fn test_different_projects_same_var_name_no_error() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr p1 [\n\
    url = `u1`\n\
    dir = `d1`\n\
] {\n\
    var string x = `from-p1`;\n\
}\n\
pr p2 [\n\
    url = `u2`\n\
    dir = `d2`\n\
] {\n\
    var string x = `from-p2`;\n\
}\
",
    );
    compile_full(&dir.path().join("main.kiru")).unwrap();
}

#[test]
fn test_sibling_case_arms_same_var_name_no_error() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    var string os = `Linux`;\n\
    fn deploy {\n\
        case $os {\n\
            `Linux` { var string x = `matched`; log $x; };\n\
            _ { var string x = `default`; log $x; };\n\
        };\n\
    }\n\
}\
",
    );
    compile_full(&dir.path().join("main.kiru")).unwrap();
}

// --- Error cases for shadowing within a chain ---

#[test]
fn test_project_var_then_fn_var_shadow_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    var string x = `project`;\n\
    fn bad {\n\
        var string x = `fn`;\n\
    }\n\
}\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(
        err.to_string().contains("$x is already defined"),
        "got: {}",
        err
    );
}

#[test]
fn test_fn_var_then_case_var_shadow_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    var string os = `Linux`;\n\
    fn bad {\n\
        var string x = `fn`;\n\
        case $os {\n\
            `Linux` { var string x = `arm`; };\n\
            _ { };\n\
        };\n\
    }\n\
}\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(
        err.to_string().contains("$x is already defined"),
        "got: {}",
        err
    );
}

#[test]
fn test_duplicate_var_in_fn_body_errors() {
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
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(
        err.to_string().contains("$x is already defined"),
        "got: {}",
        err
    );
}

// --- var shell working directory tests ---

#[test]
fn test_project_scope_var_shell_uses_project_dir() {
    let _guard = KiruCwdGuard::with_project_dir();

    let dir = tempfile::TempDir::new().unwrap();
    let subdir = dir.path().join("myproject");
    std::fs::create_dir(&subdir).unwrap();

    write_config(
        dir.path(),
        "main.kiru",
        &format!(
            "\
pr test [\n\
    url = `http://example.com`\n\
    dir = `{}`\n\
] {{\n\
    var shell cwd = `pwd`;\n\
    fn check {{\n\
        log $cwd;\n\
    }}\n\
}}\n\
",
            subdir.to_string_lossy()
        ),
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
    let fn_body = &proj.functions["check"];
    assert_eq!(fn_body.len(), 1);
    match &fn_body[0] {
        ResolvedFnStmt::Log { value } => {
            let expected = std::fs::canonicalize(&subdir)
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert_eq!(*value, expected);
        }
        other => panic!("expected Log, got {:?}", other),
    }
}

#[test]
fn test_fn_scope_var_shell_uses_project_dir() {
    let _guard = KiruCwdGuard::with_project_dir();
    let dir = tempfile::TempDir::new().unwrap();
    let subdir = dir.path().join("myproject");
    std::fs::create_dir(&subdir).unwrap();

    write_config(
        dir.path(),
        "main.kiru",
        &format!(
            "\
pr test [\n\
    url = `http://example.com`\n\
    dir = `{}`\n\
] {{\n\
    fn check {{\n\
        var shell cwd = `pwd`;\n\
        log $cwd;\n\
    }}\n\
}}\n\
",
            subdir.to_string_lossy()
        ),
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
    let fn_body = &proj.functions["check"];
    assert_eq!(fn_body.len(), 1); // VarDecl consumed, only log emitted
    match &fn_body[0] {
        ResolvedFnStmt::Log { value } => {
            let expected = std::fs::canonicalize(&subdir)
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert_eq!(*value, expected);
        }
        other => panic!("expected Log, got {:?}", other),
    }
}

#[test]
fn test_global_var_shell_uses_current_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
var shell msg = `echo hello-from-global`;\n\
pr test [\n\
    url = $msg\n\
    dir = `d`\n\
] {\n\
    fn check { log $msg; }\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
    assert_eq!(proj.url, "hello-from-global");
    let fn_body = &proj.functions["check"];
    match &fn_body[0] {
        ResolvedFnStmt::Log { value } => {
            assert_eq!(value, "hello-from-global");
        }
        other => panic!("expected Log, got {:?}", other),
    }
}

#[test]
fn test_var_shell_used_in_dir_field_no_deadlock() {
    let _guard = KiruCwdGuard::with_project_dir();
    let dir = tempfile::TempDir::new().unwrap();
    // The `dir` field resolves to `$x` (linear-phase value "workspace"),
    // which gets joined with the source directory.  Create that directory
    // so the re-resolved shell can spawn there.
    let resolved_dir = dir.path().join("workspace");
    std::fs::create_dir(&resolved_dir).unwrap();

    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = $x\n\
    dir = $x\n\
] {\n\
    var shell x = `echo workspace`;\n\
    fn check { log $x; }\n\
}\
",
    );
    // Must not deadlock/cycle.  The dir field uses the linear-phase value
    // of $x (current-dir shell execution), so dir resolves to a relative
    // path that is joined with the source file's directory.
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
    assert!(proj.url.contains("workspace"));
    assert!(proj.dir.contains("workspace"));
    // The re-resolved x (in project dir) is also "workspace" because
    // `echo` doesn't depend on working directory.
    let fn_body = &proj.functions["check"];
    match &fn_body[0] {
        ResolvedFnStmt::Log { value } => {
            assert_eq!(value, "workspace");
        }
        other => panic!("expected Log, got {:?}", other),
    }
}

#[test]
fn test_project_var_shell_runs_once() {
    let _guard = KiruCwdGuard::with_project_dir();
    let dir = tempfile::TempDir::new().unwrap();
    let subdir = dir.path().join("myproject");
    std::fs::create_dir(&subdir).unwrap();
    let marker = subdir.join("run_count.txt");

    write_config(
        dir.path(),
        "main.kiru",
        &format!(
            "\
pr test [\n\
    url = `http://example.com`\n\
    dir = `{}`\n\
] {{\n\
    var shell x = `echo 1 >> {} && echo done`;\n\
    fn check {{\n\
        log $x;\n\
    }}\n\
}}\n\
",
            subdir.to_string_lossy(),
            marker.to_string_lossy(),
        ),
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
    let fn_body = &proj.functions["check"];
    assert_eq!(fn_body.len(), 1);
    match &fn_body[0] {
        ResolvedFnStmt::Log { value } => {
            assert_eq!(value, "done");
        }
        other => panic!("expected Log, got {:?}", other),
    }
    let count = std::fs::read_to_string(&marker).unwrap();
    assert_eq!(
        count.lines().count(),
        1,
        "var shell should execute exactly once, got {} lines",
        count.lines().count()
    );
}

// --- env block var participates in enclosing fn frame ---

#[test]
fn test_env_var_participates_in_enclosing_fn() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    fn deploy {\n\
        env [MY_VAR = `hello`] {\n\
            var string x = `inside-env`;\n\
        };\n\
        log $x;\n\
    }\n\
}\
",
    );
    compile_full(&dir.path().join("main.kiru")).unwrap();
}

#[test]
fn test_env_var_redeclare_in_enclosing_fn_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
pr test [\n\
    url = `u`\n\
    dir = `d`\n\
] {\n\
    fn deploy {\n\
        var string x = `a`;\n\
        env [MY_VAR = `hello`] {\n\
            var string x = `b`;\n\
        };\n\
    }\n\
}\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(
        err.to_string().contains("$x is already defined"),
        "got: {}",
        err
    );
}
