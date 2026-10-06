//! Engine tests. They call functions directly instead of running the entry,
//! so no command output is captured. Only the tests that build commands
//! spawn processes.

use std::time::{Duration, Instant};

use crate::compiler::{checked_program, prune_unreachable};
use crate::model::Value;

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
            "module values are evaluated once at startup",
            "txt base = \"a\";\ntxt derived = base + \"b\";\nfn f() -> txt { return(derived + base); };\nfn main() {};",
            "f",
            "aba",
        ),
    ]);
}

#[test]
fn text_natives_evaluate() {
    expect_text_results(&[
        (
            "length counts bytes",
            "fn f() -> txt { return(std::text::len(\"abc\")); };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "length counts bytes, not characters",
            "fn f() -> txt { return(std::text::len(\"é\")); };\nfn main() {};",
            "f",
            "2",
        ),
        (
            "slice takes a half-open byte range",
            "fn f() -> txt { return(std::text::slice(\"abcdef\", \"1\", \"4\")); };\nfn main() {};",
            "f",
            "bcd",
        ),
        (
            "find returns a byte index",
            "fn f() -> txt { return(std::text::find(\"hello\", \"l\")); };\nfn main() {};",
            "f",
            "2",
        ),
        (
            "find returns empty when absent",
            "fn f() -> txt { return(std::text::find(\"hello\", \"z\")); };\nfn main() {};",
            "f",
            "",
        ),
        (
            "replace replaces every occurrence",
            "fn f() -> txt { return(std::text::replace(\"a-b-c\", \"-\", \"+\")); };\nfn main() {};",
            "f",
            "a+b+c",
        ),
        (
            "replace with an empty needle wraps every character",
            "fn f() -> txt { return(std::text::replace(\"abc\", \"\", \"-\")); };\nfn main() {};",
            "f",
            "-a-b-c-",
        ),
        (
            "replace with no match leaves the text",
            "fn f() -> txt { return(std::text::replace(\"abc\", \"z\", \"-\")); };\nfn main() {};",
            "f",
            "abc",
        ),
        (
            "starts_with answers text",
            "fn f() -> txt { return(std::text::starts_with(\"hello\", \"he\")); };\nfn main() {};",
            "f",
            "true",
        ),
        (
            "ends_with answers text",
            "fn f() -> txt { return(std::text::ends_with(\"hello\", \"lo\")); };\nfn main() {};",
            "f",
            "true",
        ),
        (
            "ends_with is false for a non-matching suffix",
            "fn f() -> txt { return(std::text::ends_with(\"hello\", \"he\")); };\nfn main() {};",
            "f",
            "false",
        ),
        (
            "ends_with is true for an empty suffix",
            "fn f() -> txt { return(std::text::ends_with(\"abc\", \"\")); };\nfn main() {};",
            "f",
            "true",
        ),
        (
            "ends_with on empty text needs an empty suffix",
            "fn f() -> txt { return(std::text::ends_with(\"\", \"x\")); };\nfn main() {};",
            "f",
            "false",
        ),
        (
            "ends_with on two empty texts",
            "fn f() -> txt { return(std::text::ends_with(\"\", \"\")); };\nfn main() {};",
            "f",
            "true",
        ),
        (
            "contains answers text",
            "fn f() -> txt { return(std::text::contains(\"hello\", \"ell\")); };\nfn main() {};",
            "f",
            "true",
        ),
        (
            "trim removes surrounding whitespace",
            "fn f() -> txt { return(std::text::trim(\"  hi\\n\")); };\nfn main() {};",
            "f",
            "hi",
        ),
    ]);
}

#[test]
fn slice_outside_the_text_fails_the_run() {
    let runtime = runtime(
        "fn f() -> txt { return(std::text::slice(\"abc\", \"1\", \"9\")); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the failure");
}

#[test]
fn the_kiru_quote_escapes_an_embedded_quote() {
    let runtime =
        runtime("fn f() -> txt { return(std::process::quote(\"a'b\")); };\nfn main() {};");
    assert_eq!(call_text(&runtime, "f"), "'a'\\''b'");
}

#[test]
fn file_natives_round_trip() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("data");
    let runtime = runtime(
        "fn write_it(txt path) { std::fs::write(path, \"a\"); std::fs::append(path, \"b\"); };\n\
         fn read_it(txt path) -> txt { return(std::fs::read(path)); };\n\
         fn has(txt path) -> txt { return(std::fs::exists(path)); };\n\
         fn remove_it(txt path) { std::fs::remove(path); };\n\
         fn main() {};",
    );
    let path = Value::Text(path.display().to_string());
    runtime
        .call_root("write_it", vec![path.clone()])
        .expect("the file writes");
    assert_eq!(
        runtime
            .call_root("read_it", vec![path.clone()])
            .expect("the file reads"),
        Value::Text("ab".to_owned())
    );
    assert_eq!(
        runtime
            .call_root("has", vec![path.clone()])
            .expect("exists answers"),
        Value::Text("true".to_owned())
    );
    runtime
        .call_root("remove_it", vec![path.clone()])
        .expect("the file removes");
    assert_eq!(
        runtime
            .call_root("has", vec![path])
            .expect("exists answers"),
        Value::Text("false".to_owned())
    );
}

#[test]
fn temp_file_creates_a_unique_file() {
    let runtime = runtime(
        "fn make() -> txt { return(std::fs::temp_file()); };\n\
         fn drop_it(txt path) { std::fs::remove(path); };\n\
         fn main() {};",
    );
    let first = call_text(&runtime, "make");
    let second = call_text(&runtime, "make");
    assert_ne!(first, second, "two temp paths differ");
    assert!(
        std::path::Path::new(&first).exists(),
        "the first file exists"
    );
    assert!(
        std::path::Path::new(&second).exists(),
        "the second file exists"
    );
    runtime
        .call_root("drop_it", vec![Value::Text(first.clone())])
        .expect("the first removes");
    assert!(
        !std::path::Path::new(&first).exists(),
        "the first file is gone"
    );
    runtime
        .call_root("drop_it", vec![Value::Text(second)])
        .expect("the second removes");
}

#[test]
fn capture_returns_the_commands_stdout() {
    let runtime = runtime(
        "fn f() -> txt { rec r = std::process::capture(\"sh -c 'printf hello; printf oops >&2; exit 4'\"); return(r.out + \":\" + r.err + \":\" + r.code); };\nfn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "hello:oops:4");
}

#[test]
fn lists_and_for_evaluate() {
    expect_text_results(&[
        (
            "a list literal joins",
            "fn f() -> txt { return(std::text::join([\"a\", \"b\"], \"-\")); };\nfn main() {};",
            "f",
            "a-b",
        ),
        (
            "split then join",
            "fn f() -> txt { return(std::text::join(std::text::split(\"a,b,c\", \",\"), \"-\")); };\nfn main() {};",
            "f",
            "a-b-c",
        ),
        (
            "lines drops a trailing newline",
            "fn f() -> txt { return(std::text::len(std::text::join(std::text::lines(\"a\\nb\\n\"), \"|\"))); };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "for accumulates over a list",
            "fn f() -> txt { mut txt out = \"\"; for x in [\"a\", \"b\", \"c\"] { out = out + x; }; return(out); };\nfn main() {};",
            "f",
            "abc",
        ),
        (
            "break ends the loop",
            "fn f() -> txt { mut txt out = \"\"; for x in [\"a\", \"b\", \"c\"] { out = out + x; break; }; return(out); };\nfn main() {};",
            "f",
            "a",
        ),
        (
            "a switch default skips an element",
            "fn f() -> txt { mut txt out = \"\"; for x in [\"a\", \"b\", \"c\"] { switch(x) { case(\"b\") {}; default { out = out + x; }; }; }; return(out); };\nfn main() {};",
            "f",
            "ac",
        ),
        (
            "an unbounded for breaks",
            "fn f() -> txt { mut txt out = \"x\"; for { out = \"y\"; break; }; return(out); };\nfn main() {};",
            "f",
            "y",
        ),
        (
            "an empty list runs the body zero times",
            "fn f() -> txt { mut txt out = \"x\"; for x in [] { out = \"y\"; }; return(out); };\nfn main() {};",
            "f",
            "x",
        ),
        (
            "a nested loop over a split",
            "fn f() -> txt { mut txt last = \"\"; for row in std::text::split(\"1;2;3\", \";\") { for cell in std::text::split(row, \";\") { last = cell; }; }; return(last); };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "a module list value is read",
            "list xs = [\"a\", \"b\"];\nfn f() -> txt { return(std::text::join(xs, \"\")); };\nfn main() {};",
            "f",
            "ab",
        ),
    ]);
}

#[test]
fn glob_lists_matching_paths() {
    let directory = tempfile::tempdir().expect("temp dir");
    std::fs::write(directory.path().join("a.txt"), "").expect("write a");
    std::fs::write(directory.path().join("b.txt"), "").expect("write b");
    std::fs::write(directory.path().join("c.log"), "").expect("write c");
    let pattern = format!("{}/*.txt", directory.path().display());
    let runtime = runtime(&format!(
        "fn f() -> txt {{ return(std::text::join(std::fs::glob(\"{pattern}\"), \"|\")); }};\nfn main() {{}};",
        pattern = pattern
    ));
    let result = call_text(&runtime, "f");
    let a = result.find("a.txt").expect("a.txt is a match");
    let b = result.find("b.txt").expect("b.txt is a match");
    assert!(a < b, "glob returns sorted matches: {result}");
    assert!(!result.contains("c.log"), "{result}");
}

#[test]
fn spawn_and_wait_for_run_an_argv_without_a_shell() {
    expect_text_results(&[
        (
            "an argv exits zero",
            "fn f() -> txt { txt pid = std::process::spawn({ Stdout = \"null\" }, [\"true\"]); return(std::process::wait_for(pid)); };\nfn main() {};",
            "f",
            "0",
        ),
        (
            "an argv exit code comes back",
            "fn f() -> txt { txt pid = std::process::spawn({ Stdout = \"null\" }, [\"false\"]); return(std::process::wait_for(pid)); };\nfn main() {};",
            "f",
            "1",
        ),
    ]);
}

#[test]
fn spawn_runs_in_its_directory() {
    let directory = tempfile::tempdir().expect("temp dir");
    let source = format!(
        "fn f() -> txt {{ txt pid = std::process::spawn({{ Dir = \"{}\" }}, [\"touch\", \"made\"]); return(std::process::wait_for(pid)); }};\nfn main() {{}};",
        directory.path().display()
    );
    let runtime = runtime(&source);
    assert_eq!(call_text(&runtime, "f"), "0");
    assert!(
        directory.path().join("made").exists(),
        "the command ran in the directory"
    );
}

#[test]
fn signal_kills_a_spawned_process() {
    let runtime = runtime(
        "fn f() -> txt { txt pid = std::process::spawn({ Stdout = \"null\" }, [\"sleep\", \"5\"]); std::process::signal(pid, \"KILL\"); return(std::process::wait_for(pid)); };\nfn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "137");
}

#[test]
fn list_operations_evaluate() {
    expect_text_results(&[
        (
            "length",
            "fn f() -> txt { return(std::lists::len([\"a\", \"b\", \"c\"])); };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "get",
            "fn f() -> txt { return(std::lists::get([\"a\", \"b\", \"c\"], \"1\")); };\nfn main() {};",
            "f",
            "b",
        ),
        (
            "append then join",
            "fn f() -> txt { return(std::text::join(std::lists::append([\"a\", \"b\"], \"c\"), \"-\")); };\nfn main() {};",
            "f",
            "a-b-c",
        ),
        (
            "contains true",
            "fn f() -> txt { return(std::lists::contains([\"a\", \"b\"], \"b\")); };\nfn main() {};",
            "f",
            "true",
        ),
        (
            "contains false",
            "fn f() -> txt { return(std::lists::contains([\"a\", \"b\"], \"z\")); };\nfn main() {};",
            "f",
            "false",
        ),
        (
            "reverse",
            "fn f() -> txt { return(std::text::join(std::lists::reverse([\"a\", \"b\", \"c\"]), \"\")); };\nfn main() {};",
            "f",
            "cba",
        ),
        (
            "last",
            "fn f() -> txt { return(std::lists::last([\"a\", \"b\", \"c\"])); };\nfn main() {};",
            "f",
            "c",
        ),
        (
            "last of an empty list is empty",
            "fn f() -> txt { return(std::lists::last([])); };\nfn main() {};",
            "f",
            "",
        ),
        (
            "concat joins two lists in order",
            "fn f() -> txt { return(std::text::join(std::lists::concat([\"a\", \"b\"], [\"c\", \"d\"]), \"\")); };\nfn main() {};",
            "f",
            "abcd",
        ),
        (
            "concat with an empty list",
            "fn f() -> txt { return(std::text::join(std::lists::concat([], [\"a\"]), \"\")); };\nfn main() {};",
            "f",
            "a",
        ),
        (
            "sort",
            "fn f() -> txt { return(std::text::join(std::lists::sort([\"c\", \"a\", \"b\"]), \"\")); };\nfn main() {};",
            "f",
            "abc",
        ),
    ]);
}

#[test]
fn path_operations_evaluate() {
    expect_text_results(&[
        (
            "join",
            "fn f() -> txt { return(std::path::join(\"/a/b\", \"c\")); };\nfn main() {};",
            "f",
            "/a/b/c",
        ),
        (
            "base",
            "fn f() -> txt { return(std::path::base(\"/a/b/c.txt\")); };\nfn main() {};",
            "f",
            "c.txt",
        ),
        (
            "dir",
            "fn f() -> txt { return(std::path::dir(\"/a/b/c.txt\")); };\nfn main() {};",
            "f",
            "/a/b",
        ),
        (
            "ext",
            "fn f() -> txt { return(std::path::ext(\"/a/b/c.txt\")); };\nfn main() {};",
            "f",
            "txt",
        ),
        (
            "is_absolute true",
            "fn f() -> txt { return(std::path::is_absolute(\"/a\")); };\nfn main() {};",
            "f",
            "true",
        ),
        (
            "is_absolute false",
            "fn f() -> txt { return(std::path::is_absolute(\"a\")); };\nfn main() {};",
            "f",
            "false",
        ),
    ]);
}

#[test]
fn env_reads_an_unset_variable_as_empty() {
    let runtime = runtime(
        "fn f() -> txt { return(std::env::var(\"KIRU_DEFINITELY_UNSET_VAR\")); };\nfn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "");
}

#[test]
fn env_reports_a_current_directory() {
    let runtime = runtime("fn f() -> txt { return(std::env::current_dir()); };\nfn main() {};");
    assert!(!call_text(&runtime, "f").is_empty());
}

#[test]
fn status_code_reads_a_number_or_a_message() {
    assert_eq!(super::status_code("0"), 0);
    assert_eq!(super::status_code("42"), 42);
    assert_eq!(super::status_code("300"), 255);
    assert_eq!(super::status_code("boom"), 1);
}

#[test]
fn a_list_get_that_misses_reads_empty() {
    let runtime =
        runtime("fn f() -> txt { return(std::lists::get([\"a\"], \"9\")); };\nfn main() {};");
    assert_eq!(call_text(&runtime, "f"), "");
}

#[test]
fn list_contains_evaluates() {
    expect_text_results(&[
        (
            "present",
            "fn f() -> txt { return(std::lists::contains([\"a\", \"b\"], \"b\")); };\nfn main() {};",
            "f",
            "true",
        ),
        (
            "absent",
            "fn f() -> txt { return(std::lists::contains([\"a\", \"b\"], \"z\")); };\nfn main() {};",
            "f",
            "false",
        ),
    ]);
}

#[test]
fn env_temp_dir_is_nonempty() {
    let runtime = runtime("fn f() -> txt { return(std::env::temp_dir()); };\nfn main() {};");
    assert!(!call_text(&runtime, "f").is_empty());
}

#[test]
fn command_accepts_a_null_stream() {
    let runtime = runtime(
        "fn f() -> txt { return(std::process::command({ Stdout = \"null\", Stderr = \"null\" }, \"echo hidden; true\")); };\nfn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "0");
}

#[test]
fn command_rejects_an_unknown_stream_mode() {
    let runtime = runtime(
        "fn f() -> txt { return(std::process::command({ Stdout = \"hide\" }, \"true\")); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the failure");
}

#[test]
fn signal_rejects_an_unknown_name() {
    let runtime = runtime(
        "fn f() -> txt { txt pid = std::process::spawn({ Stdout = \"null\" }, [\"true\"]); std::process::signal(pid, \"NOPE\"); return(std::process::wait_for(pid)); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
}

#[test]
fn sleep_rejects_non_numeric_input() {
    let runtime = runtime("fn f() { std::time::sleep(\"soon\"); };\nfn main() {};");
    assert!(runtime.call_root("f", Vec::new()).is_err());
}

#[test]
fn write_rejects_an_unknown_file_descriptor() {
    let runtime = runtime("fn f() { std::io::write(\"9\", \"x\"); };\nfn main() {};");
    assert!(runtime.call_root("f", Vec::new()).is_err());
}

#[test]
fn sleep_returns_nothing() {
    let runtime = runtime("fn f() { std::time::sleep(\"0\"); };\nfn main() {};");
    let value = runtime.call_root("f", Vec::new()).expect("sleep succeeds");
    assert_eq!(value, Value::Nothing);
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
           mut txt found = \"\";\n\
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
fn command_returns_the_exit_code() {
    let runtime =
        runtime("fn f() -> txt { return(std::process::command({}, \"exit 3\")); };\nfn main() {};");
    assert_eq!(call_text(&runtime, "f"), "3");
}

#[test]
fn command_returns_zero_on_success() {
    let runtime =
        runtime("fn f() -> txt { return(std::process::command({}, \"true\")); };\nfn main() {};");
    assert_eq!(call_text(&runtime, "f"), "0");
}

#[test]
fn a_bare_command_statement_runs() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("ran");
    let source = format!(
        "fn f() -> txt {{ std::process::command({{}}, \"touch {}\"); return(\"ok\"); }};\nfn main() {{}};",
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
        "fn worker() {{ std::process::command({{}}, \"sleep 0.2; touch {}\"); }};\n\
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
        "fn work(txt marker) {{ std::process::command({{}}, \"sleep 0.2; touch \" + marker); }};\n\
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
        "fn child() {{ std::process::command({{}}, \"sleep 0.2; touch {}\"); }};\n\
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
fn panic_in_a_waited_thread_fails_the_run() {
    let runtime = runtime(
        "fn worker() { panic; };\n\
         fn f() -> txt { async worker(); wait; return(\"done\"); };\nfn main() {};",
    );
    let _ = runtime.call_root("f", Vec::new());
    assert!(runtime.state.panicked(), "the joined panic fails the run");
}

#[test]
fn async_over_a_command_call_runs_on_the_thread() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("ran");
    let source = format!(
        "fn f() -> txt {{\n\
           async std::process::command({{}}, \"touch {}\");\n\
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
           async std::process::command({{ Dir = \"{}\" }}, \"touch ran\");\n\
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
fn a_panic_cancels_other_threads_commands() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("survived");
    let source = format!(
        "fn worker() {{ panic; }};\n\
         fn other() {{ std::process::command({{}}, \"sleep 0.2; touch {}\"); }};\n\
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
    let _ = runtime.call_root("f", Vec::new());
    assert!(runtime.state.panicked(), "the panic is recorded");
    assert!(
        !marker.exists(),
        "a panic cancels the other thread's command"
    );
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
