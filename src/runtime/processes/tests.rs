//! Concurrency and process tests: `async`, `wait`, and the process kernel.
//!
//! These tests spawn threads and commands, so they drive the run state
//! directly instead of running the entry.

use std::time::{Duration, Instant};

use crate::compiler::checked_program_and_entry;
use crate::runtime::Runtime;
use crate::runtime::Value;

fn runtime(source: &str) -> Runtime {
    let (program, facts, entry) = checked_program_and_entry(source);
    Runtime::for_testing(program, facts, entry)
}

/// The process group table grows with the operating system's resources: no
/// fixed slot count, so no spawn fails for filling it.
#[test]
fn the_process_group_table_has_no_fixed_cap() {
    let state = super::state::RuntimeState::new();
    for pgid in 1..=4096 {
        state.processes.register(pgid);
    }
    assert_eq!(state.processes.group_count(), 4096);
    state.processes.unregister(2048);
    assert_eq!(state.processes.group_count(), 4095);
}

fn call_text(runtime: &Runtime, name: &str) -> String {
    match runtime
        .call_root(name, Vec::new())
        .expect("the call succeeds")
    {
        Value::Text(text) => text.as_str().to_owned(),
        other => panic!("expected text, found {other:?}"),
    }
}

#[test]
fn async_does_not_wait_for_the_call() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn worker() -> txt { return \"w\"; };\n\
         fn f() -> txt { async worker(); return \"done\"; };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_blocks_until_the_thread_finishes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("finished");
    let source = format!(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn worker() {{ std::process::command(\"sleep 0.2; touch {}\").run(); }};\n\
         fn f() -> txt {{ async worker(); wait; return \"done\"; }};\n\
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
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn work(marker: txt) {{ std::process::command(\"sleep 0.2; touch @(marker)\").run(); }};\n\
         fn f() -> txt {{\n\
           async work(\"{first}\");\n\
           async work(\"{second}\");\n\
           async work(\"{third}\");\n\
           wait;\n\
           return \"done\";\n\
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
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn worker() {};\n\
         fn f() -> txt {\n\
           async worker();\n\
           wait;\n\
           wait;\n\
           return \"done\";\n\
         };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_inside_a_spawned_thread_returns_without_joining_its_spawner() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn worker() { wait; };\n\
         fn f() -> txt { async worker(); wait; return \"done\"; };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_inside_a_spawned_thread_joins_its_own_children() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("finished");
    let source = format!(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn child() {{ std::process::command(\"sleep 0.2; touch {}\").run(); }};\n\
         fn worker() {{ async child(); wait; }};\n\
         fn f() -> txt {{ async worker(); wait; return \"done\"; }};\n\
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
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn worker() { panic; };\n\
         fn f() -> txt { async worker(); return \"done\"; };\n\
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
fn panic_in_a_waited_thread_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn worker() { panic; };\n\
         fn f() -> txt { async worker(); wait; return \"done\"; };\nfn main() {};",
    );
    let _ = runtime.call_root("f", Vec::new());
    assert!(runtime.state.panicked(), "the joined panic fails the run");
}

#[test]
fn async_over_a_command_call_runs_on_the_thread() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("ran");
    let source = format!(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt {{\n\
           async std::process::command(\"touch {}\").run();\n\
           wait;\n\
           return \"done\";\n\
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
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt {{\n\
           async std::process::command(\"touch ran\").dir(\"{}\").run();\n\
           wait;\n\
           return \"done\";\n\
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
fn a_panic_cancels_other_threads_commands() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("survived");
    let source = format!(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn worker() {{ panic; }};\n\
         fn other() {{ std::process::command(\"sleep 0.2; touch {}\").run(); }};\n\
         fn f() -> txt {{\n\
           async worker();\n\
           async other();\n\
           wait;\n\
           return \"done\";\n\
         }};\n\
         fn main() {{}};",
        marker.display()
    );
    let runtime = runtime(&source);
    let _ = runtime.call_root("f", Vec::new());
    assert!(runtime.state.panicked(), "the panic is recorded");
    assert!(
        !marker.exists(),
        "a panic cancels the other thread's command"
    );
}
