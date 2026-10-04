//! Engine tests. They call functions directly instead of running the entry,
//! so no command output is captured. Only the tests that build commands
//! spawn processes.

use std::time::{Duration, Instant};

use crate::compiler::{Value, checked_program, prune_unreachable};

use super::Runtime;

fn runtime(source: &str) -> Runtime {
    Runtime::for_testing(checked_program(source))
}

fn call_text(runtime: &Runtime, name: &str) -> String {
    match runtime
        .call_root(name, Vec::new())
        .expect("the call succeeds")
    {
        Value::Text(text) => text,
        other => panic!("expected text, found {other:?}"),
    }
}

/// Run every pure evaluation case, collecting all mismatches so one run
/// reports every case that failed.
fn expect_text_results(cases: &[(&str, &str, &str, &str)]) {
    let mut failures = Vec::new();
    for &(name, source, function, expected) in cases {
        let actual = call_text(&runtime(source), function);
        if actual != expected {
            failures.push(format!("{name}: expected `{expected}`, found `{actual}`"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} evaluation case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn args_record_holds_two_verbatim_words() {
    let record = super::build_arguments_record(&["deploy".to_owned(), "fast".to_owned()])
        .expect("two words");
    assert_eq!(record.get("cmd"), "deploy");
    assert_eq!(record.get("flag"), "fast");
    assert_eq!(record.get("missing"), "");

    let absent = super::build_arguments_record(&[]).expect("no words");
    assert_eq!(absent.get("cmd"), "");
    assert_eq!(absent.get("flag"), "");

    let extra = super::build_arguments_record(&["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    assert!(extra.is_err());
}

#[test]
fn evaluates_expressions() {
    expect_text_results(&[
        (
            "returns a text value",
            "fn answer() -> txt { return(\"42\"); };\nfn main() {};",
            "answer",
            "42",
        ),
        (
            "missing field reads empty",
            "fn f() -> txt { rec r = { a = \"1\" }; return(r.b + \"!\"); };\nfn main() {};",
            "f",
            "!",
        ),
        (
            "duplicate key last wins",
            "fn f() -> txt { rec r = { a = \"1\", a = \"2\" }; return(r.a); };\nfn main() {};",
            "f",
            "2",
        ),
        (
            "defer does not change the returned value",
            "fn f() -> txt { txt x = \"a\"; defer { x = \"b\"; }; return(x); };\nfn main() {};",
            "f",
            "a",
        ),
        (
            "defer body uses its own local",
            "fn f() -> txt { txt x = \"a\"; defer { txt y = \"b\"; x = y; }; return(x); };\nfn main() {};",
            "f",
            "a",
        ),
        (
            "module values are evaluated once at startup",
            "txt base = \"a\";\ntxt derived = base + \"b\";\nfn f() -> txt { return(derived + base); };\nfn main() {};",
            "f",
            "aba",
        ),
    ]);
}

#[test]
fn a_void_call_produces_nothing() {
    let runtime = runtime("fn work() {};\nfn main() {};");
    let value = runtime
        .call_root("work", Vec::new())
        .expect("the call succeeds");
    assert_eq!(value, Value::Nothing);
}

#[test]
fn switch_takes_the_first_match_then_default() {
    let runtime = runtime(
        "txt a = \"a\";\n\
         fn f(txt s) -> txt {\n\
           txt found = \"\";\n\
           switch(s) {\n\
             case(\"a\") { found = \"one\"; };\n\
             case(a) { found = \"two\"; };\n\
             default { found = \"other\"; };\n\
           };\n\
           return(found);\n\
         };\n\
         fn main() {};",
    );
    let first = runtime
        .call_root("f", vec![Value::Text("a".to_owned())])
        .expect("the call succeeds");
    assert_eq!(first, Value::Text("one".to_owned()));
    let fallback = runtime
        .call_root("f", vec![Value::Text("z".to_owned())])
        .expect("the call succeeds");
    assert_eq!(fallback, Value::Text("other".to_owned()));
}

#[test]
fn run_returns_the_exit_code() {
    let runtime = runtime("fn f() -> txt { return(std::run(\"exit 3\")); };\nfn main() {};");
    assert_eq!(call_text(&runtime, "f"), "3");
}

#[test]
fn run_returns_zero_on_success() {
    let runtime = runtime("fn f() -> txt { return(std::run(\"true\")); };\nfn main() {};");
    assert_eq!(call_text(&runtime, "f"), "0");
}

#[test]
fn a_bare_run_statement_runs() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("ran");
    let source = format!(
        "fn f() -> txt {{ std::run(\"touch {}\"); return(\"ok\"); }};\nfn main() {{}};",
        marker.display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "ok");
    assert!(marker.exists(), "the bare command ran");
}

#[test]
fn panic_stops_the_run() {
    let runtime = runtime("fn f() { panic; };\nfn main() {};");
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the panic");
}

#[test]
fn panic_runs_pending_defers() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("cleaned");
    let source = format!(
        "fn f() {{\n\
           defer {{ std::run(\"touch {}\"); }};\n\
           panic;\n\
         }};\nfn main() {{}};",
        marker.display()
    );
    let runtime = runtime(&source);
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(marker.exists(), "the deferred cleanup command ran");
}

#[test]
fn async_does_not_wait_for_the_call() {
    let runtime = runtime(
        "fn worker() -> txt { return(\"w\"); };\n\
         fn f() -> txt { async worker(); return(\"done\"); };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_blocks_until_the_thread_finishes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("finished");
    let source = format!(
        "fn worker() {{ std::run(\"sleep 0.2; touch {}\"); }};\n\
         fn f() -> txt {{ async worker(); wait; return(\"done\"); }};\n\
         fn main() {{}};",
        marker.display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(marker.exists(), "wait joined the thread before continuing");
}

#[test]
fn wait_joins_every_outstanding_thread() {
    let directory = tempfile::tempdir().expect("temp dir");
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    let third = directory.path().join("third");
    let source = format!(
        "fn work(txt marker) {{ std::run(\"sleep 0.2; touch \" + marker); }};\n\
         fn f() -> txt {{\n\
           async work(\"{first}\");\n\
           async work(\"{second}\");\n\
           async work(\"{third}\");\n\
           wait;\n\
           return(\"done\");\n\
         }};\nfn main() {{}};",
        first = first.display(),
        second = second.display(),
        third = third.display(),
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(first.exists(), "the first thread finished");
    assert!(second.exists(), "the second thread finished");
    assert!(third.exists(), "the third thread finished");
}

#[test]
fn a_second_wait_returns_immediately() {
    let runtime = runtime(
        "fn worker() {};\n\
         fn f() -> txt {\n\
           async worker();\n\
           wait;\n\
           wait;\n\
           return(\"done\");\n\
         };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_inside_a_spawned_thread_returns_without_joining_its_spawner() {
    let runtime = runtime(
        "fn worker() { wait; };\n\
         fn f() -> txt { async worker(); wait; return(\"done\"); };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_inside_a_spawned_thread_joins_its_own_children() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("finished");
    let source = format!(
        "fn child() {{ std::run(\"sleep 0.2; touch {}\"); }};\n\
         fn worker() {{ async child(); wait; }};\n\
         fn f() -> txt {{ async worker(); wait; return(\"done\"); }};\n\
         fn main() {{}};",
        marker.display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(
        marker.exists(),
        "the inner wait joined the thread worker spawned"
    );
}

#[test]
fn panic_in_an_async_thread_sets_the_panicked_flag() {
    let runtime = runtime(
        "fn worker() { panic; };\n\
         fn f() -> txt { async worker(); return(\"done\"); };\n\
         fn main() {};",
    );
    // The detached panic can reach the caller before or after `f` returns.
    let _ = runtime.call_root("f", Vec::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !runtime.state.panicked() {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        runtime.state.panicked(),
        "a panic in a detached thread is remembered"
    );
}

#[test]
fn panic_in_a_waited_thread_is_remembered_without_stopping_the_caller() {
    let runtime = runtime(
        "fn worker() { panic; };\n\
         fn f() -> txt {\n\
           async worker();\n\
           wait;\n\
           return(\"done\");\n\
         };\n\
         fn main() {};",
    );
    let value = runtime
        .call_root("f", Vec::new())
        .expect("a joined panic does not unwind the caller");
    assert_eq!(value, Value::Text("done".to_owned()));
    assert!(
        runtime.state.panicked(),
        "the joined panic still fails the run"
    );
}

#[test]
fn async_over_a_run_call_runs_on_the_thread() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("ran");
    let source = format!(
        "fn f() -> txt {{\n\
           async std::run(\"touch {}\");\n\
           wait;\n\
           return(\"done\");\n\
         }};\n\
         fn main() {{}};",
        marker.display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(marker.exists(), "the call ran on the spawned thread");
}

#[test]
fn async_of_a_command_call_runs_in_its_directory() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("ran");
    let source = format!(
        "fn f() -> txt {{\n\
           async std::command({{ Dir = \"{}\" }}, \"touch ran\");\n\
           wait;\n\
           return(\"done\");\n\
         }};\n\
         fn main() {{}};",
        directory.path().display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(
        marker.exists(),
        "the command ran in the directory named by the spec"
    );
}

#[test]
fn a_panic_in_one_thread_does_not_cancel_another_threads_command() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("survived");
    let source = format!(
        "fn worker() {{ panic; }};\n\
         fn other() {{ std::run(\"sleep 0.2; touch {}\"); }};\n\
         fn f() -> txt {{\n\
           async worker();\n\
           async other();\n\
           wait;\n\
           return(\"done\");\n\
         }};\n\
         fn main() {{}};",
        marker.display()
    );
    let runtime = runtime(&source);
    assert_eq!(call_text(&runtime, "f"), "done");
    assert!(runtime.state.panicked(), "the panic is still recorded");
    assert!(
        marker.exists(),
        "a panic in one thread must not cancel another thread's command"
    );
}

#[test]
fn a_panic_in_a_defer_does_not_stop_the_other_defers() {
    let directory = tempfile::tempdir().expect("temp dir");
    let first = directory.path().join("first");
    let last = directory.path().join("last");
    let runtime = runtime(&format!(
        "fn f() -> txt {{\n\
           defer {{ std::run(\"touch {first}\"); }};\n\
           defer {{ panic; }};\n\
           defer {{ std::run(\"touch {last}\"); }};\n\
           return(\"\");\n\
         }};\n\
         fn main() {{}};",
        first = first.display(),
        last = last.display(),
    ));
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(last.exists(), "the defer after the panic still runs");
    assert!(first.exists(), "the first registered defer runs last");
}

#[test]
fn a_retained_program_still_runs() {
    let mut program = checked_program(
        "fn answer() -> txt { return(\"42\"); };\n\
         fn main() { answer(); };",
    );
    prune_unreachable(&mut program);
    let runtime = Runtime::for_testing(program);
    let value = runtime
        .call_root("answer", Vec::new())
        .expect("the retained function runs");
    assert_eq!(value, Value::Text("42".to_owned()));
}

#[test]
fn retained_module_values_still_evaluate() {
    let mut program = checked_program(
        "txt base = \"a\";\n\
         fn f() -> txt { return(base + \"b\"); };\n\
         fn main() { f(); };",
    );
    prune_unreachable(&mut program);
    let runtime = Runtime::for_testing(program);
    let value = runtime
        .call_root("f", Vec::new())
        .expect("the retained module value evaluates");
    assert_eq!(value, Value::Text("ab".to_owned()));
}
