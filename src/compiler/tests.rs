use super::*;
use crate::runner::Runner;
use std::fs;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

/// Serializes tests that read or modify the SANCTUARY env var.
static SANCTUARY_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Run a closure with SANCTUARY set to `val`, restoring the original value
/// afterward. Holds SANCTUARY_MUTEX across the call and uses catch_unwind
/// so that a panic inside f() neither leaks the modified env var nor poisons
/// the mutex.
fn with_sanctuary<R>(val: &str, f: impl FnOnce() -> R) -> R {
    let _guard = SANCTUARY_MUTEX.lock().unwrap();
    let prev = std::env::var("SANCTUARY").ok();
    // SAFETY: set_var is safe here because SANCTUARY_MUTEX prevents
    // concurrent env var access from other threads.
    unsafe {
        std::env::set_var("SANCTUARY", val);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    match prev {
        Some(v) => {
            // SAFETY: Same mutex guard still held.
            unsafe {
                std::env::set_var("SANCTUARY", v);
            }
        }
        None => {
            // SAFETY: Same mutex guard still held.
            unsafe {
                std::env::remove_var("SANCTUARY");
            }
        }
    }
    drop(_guard);
    match result {
        Ok(r) => r,
        Err(e) => std::panic::resume_unwind(e),
    }
}

/// Run a closure with SANCTUARY removed from the environment, restoring
/// the original value afterward. Holds SANCTUARY_MUTEX across the call
/// and uses catch_unwind so that a panic inside f() neither leaks the
/// modified env var nor poisons the mutex.
fn without_sanctuary<R>(f: impl FnOnce() -> R) -> R {
    let _guard = SANCTUARY_MUTEX.lock().unwrap();
    let prev = std::env::var("SANCTUARY").ok();
    // SAFETY: remove_var is safe here because SANCTUARY_MUTEX prevents
    // concurrent env var access from other threads.
    unsafe {
        std::env::remove_var("SANCTUARY");
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    if let Some(v) = prev {
        // SAFETY: Same mutex guard still held.
        unsafe {
            std::env::set_var("SANCTUARY", v);
        }
    }
    drop(_guard);
    match result {
        Ok(r) => r,
        Err(e) => std::panic::resume_unwind(e),
    }
}

fn compile_full(entry_path: &Path) -> Result<Sanctuary, CompileError> {
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
sanctuary = `/tmp/dev`;\n\
var string a = `hello`;\n\
pr test { url = `http://example.com`; dir = `test`; }\n\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.sanctuary_path, "/tmp/dev");
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
sanctuary = `/tmp`;\n\
import `./other.kiru`;\n\
pr p { url = $extra; dir = `d`; }\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.projects["p"].url, "from-other");
}

#[test]
fn test_circular_import() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "a.kiru",
        "import `./b.kiru`; sanctuary = `/tmp`;",
    );
    write_config(
        dir.path(),
        "b.kiru",
        "import `./a.kiru`; sanctuary = `/tmp`;",
    );
    let err = compile_full(&dir.path().join("a.kiru")).unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("circular") || err_str.contains("Circular"),
        "got: {}",
        err_str
    );
}

#[test]
fn test_first_sanctuary_wins() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
sanctuary = `/other`;\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.sanctuary_path, "/tmp");
}

#[test]
fn test_shadowing_global_var() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
var string x = `a`;\n\
var string x = `b`;\n\
pr p { url = $x; dir = `d`; }\
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
sanctuary = `/tmp`;\n\
pr p1 { url = `u`; dir = `d1`; }\n\
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
sanctuary = `/tmp`;\n\
var string a = `x`;\n\
var string b = $a;\n\
var string c = $b;\n\
pr p { url = $c; dir = `d`; }\
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
sanctuary = `/tmp`;\n\
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
fn test_missing_sanctuary() {
    without_sanctuary(|| {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
pr test { url = `http://example.com`; dir = `test`; }\
",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("sanctuary"), "got: {}", err);
    })
}

#[test]
fn test_sanctuary_absolute_path() {
    without_sanctuary(|| {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
sanctuary = `relative/path`;\
",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("absolute"), "got: {}", err);
    })
}

#[test]
fn test_missing_url() {
    without_sanctuary(|| {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
sanctuary = `/tmp`;\n\
pr p { dir = `d`; }\
",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("url is required"), "got: {}", err);
    })
}

#[test]
fn test_missing_dir() {
    without_sanctuary(|| {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
sanctuary = `/tmp`;\n\
pr p { url = `u`; }\
",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("dir is required"), "got: {}", err);
    })
}

#[test]
fn test_duplicate_dir() {
    without_sanctuary(|| {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
sanctuary = `/tmp`;\n\
pr a { url = `ua`; dir = `shared`; }\n\
pr b { url = `ub`; dir = `shared`; }\
",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate directory"),
            "got: {}",
            err
        );
    })
}

#[test]
fn test_invalid_sync_value() {
    without_sanctuary(|| {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
sanctuary = `/tmp`;\n\
pr p { url = `u`; dir = `d`; sync = `invalid`; }\
",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("sync"), "got: {}", err);
    })
}

#[test]
fn test_duplicate_project_field() {
    without_sanctuary(|| {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
sanctuary = `/tmp`;\n\
pr p { url = `u`; dir = `d`; dir = `e`; }\
",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("duplicate field"), "got: {}", err);
    })
}

#[test]
fn test_only_sanctuary() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.sanctuary_path, "/tmp");
}

#[test]
fn test_interpolation_in_backtick() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
var string name = `world`;\n\
pr p { url = `http://${name}.com`; dir = `d`; }\
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
sanctuary = `/tmp`;\n\
var string myurl = `http://example.com`;\n\
pr x { url = $myurl; dir = `d`; }\
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
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
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
sanctuary = `/tmp`;\n\
import `./a.kiru`;\n\
pr p { url = $a; dir = `d`; }\
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
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
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
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
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
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
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
    // Shadowing is allowed in fn bodies — latest declaration wins within its scope
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert!(cfg.projects["test"].functions.contains_key("bad"));
    // VarDecls are inlined at compile time, so the resolved body is empty
    let body = &cfg.projects["test"].functions["bad"];
    assert_eq!(body.len(), 0);
}

#[test]
fn test_sanctuary_with_var_ref() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        &format!(
            "\
var string workdir = `{}`;\n\
sanctuary = $workdir;\
",
            dir.path().display()
        ),
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert_eq!(cfg.sanctuary_path, dir.path().to_str().unwrap());
}

#[test]
fn test_project_var_chain_resolution() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
    var string a = `hello`;\n\
    var string b = $a;\n\
}\
",
    );
    // We can't check project vars directly on the resolved Sanctuary,
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
    compile_full(&dir.path().join("main.kiru")).unwrap();
}

#[test]
fn test_undefined_var_in_case_condition() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
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
sanctuary = `/tmp`;\n\
pr test {\n\
    url = `u`;\n\
    dir = `d`;\n\
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
        &format!(
            "\
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
        &format!(
            "\
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
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let mut runner = Runner::new(Arc::new(cfg));
    runner.execute_fn_call("deploy", "test").unwrap();
}

// --- Top-level fn/run collection (SANCTUARY=0 ready) ---

#[test]
fn test_top_level_fn_collection() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
fn build { log `building`; }\n\
fn test { exec `check`; }\n\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert!(cfg.functions.contains_key("build"));
    assert!(cfg.functions.contains_key("test"));
    assert_eq!(cfg.functions.len(), 2);
}

#[test]
fn test_top_level_run_collection() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
fn build { log `x`; }\n\
fn test { log `y`; }\n\
run all { build => test; }\n\
run ci { build; }\n\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert!(cfg.runs.contains_key("all"));
    assert!(cfg.runs.contains_key("ci"));
    assert_eq!(cfg.runs.len(), 2);
    assert_eq!(cfg.runs["all"], vec![vec!["build", "test"]]);
}

#[test]
fn test_top_level_duplicate_fn_first_wins() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
fn dup { log `a`; }\n\
fn dup { log `b`; }\n\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert!(cfg.functions.contains_key("dup"));
    // First-wins: first fn's body is used
    assert_eq!(cfg.functions["dup"].len(), 1);
}

#[test]
fn test_top_level_duplicate_run_first_wins() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
fn x { log `a`; }\n\
run dup { x; }\n\
run dup { x; }\n\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    assert!(cfg.runs.contains_key("dup"));
    assert_eq!(cfg.runs["dup"].len(), 1);
}

#[test]
fn test_top_level_run_validates_function_refs() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
run bad { nonexistent; }\n\
",
    );
    let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
    assert!(err.to_string().contains("unknown function"), "got: {}", err);
}

#[test]
fn test_top_level_fn_var_validation() {
    let dir = tempfile::TempDir::new().unwrap();
    write_config(
        dir.path(),
        "main.kiru",
        "\
sanctuary = `/tmp`;\n\
fn bad { log $undefined; }\n\
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
sanctuary = `/tmp`;\n\
pr test {\n\
    var string name = `myproject`;\n\
    url = `http://example.com/${name}`;\n\
    dir = $name;\n\
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
sanctuary = `/tmp`;\n\
var string name = `global`;\n\
pr test {\n\
    var string name = `project`;\n\
    url = `http://example.com`;\n\
    dir = $name;\n\
}\
",
    );
    let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
    let proj = &cfg.projects["test"];
    // Project-level var "name" shadows the global "name"
    assert_eq!(proj.dir, "project");
}

// --- SANCTUARY=0 standalone mode (env var required) ---

#[test]
fn test_standalone_config_no_sanctuary() {
    with_sanctuary("0", || {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
fn build { log `building`; }\n\
fn test { exec `check`; }\n\
run all { build => test; }\n\
",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.sanctuary_path, "");
        assert!(cfg.functions.contains_key("build"));
        assert!(cfg.functions.contains_key("test"));
        assert!(cfg.runs.contains_key("all"));
        assert!(cfg.projects.is_empty());
    })
}
