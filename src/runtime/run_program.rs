//! The interpreter: calls, expressions, module values, and defers over the
//! linked model.
//!
//! Module values are evaluated once at startup, before `main`. Names are
//! edges: a reference reads the environment of the running body or the
//! program's value table. The `async` keyword starts an OS thread; `wait`
//! joins the threads the calling thread spawned. This module owns the run
//! loop, the runtime handle, and the shared helpers; the grammar layers live
//! in the expression and statement submodules.

mod evaluate_expressions;
mod execute_statements;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use crate::compiler::{
    DeclarationId, DeclarationKind, Expression, Kind, Program, Record, RwLockExt, Statement, Value,
};
use crate::runtime::Panic;

use super::manage_processes::{self as process, RuntimeState};

/// The bindings one body runs with. A function call starts an empty
/// environment and binds its parameter nodes; nested blocks share it.
pub(super) type Environment = HashMap<DeclarationId, Value>;

/// Run a program's entry with the words after the program name. Returns the
/// process exit code: zero on success, one on failure or panic, and 130 when
/// a signal arrived.
pub(crate) fn run(program: Arc<Program>, words: &[String]) -> i32 {
    let arguments = match build_arguments_record(words) {
        Ok(record) => record,
        Err(message) => {
            eprintln!("{}: {message}", super::program_name());
            return 1;
        }
    };
    let state = Arc::new(RuntimeState::new());
    // The signal handler keeps a raw pointer to the state, so the state must
    // outlive the run, including a partially installed handler set.
    std::mem::forget(Arc::clone(&state));
    if let Err(error) = process::install_signal_handlers(&state) {
        eprintln!(
            "{}: cannot install signal handlers: {error}",
            super::program_name()
        );
        return 1;
    }
    let runtime = Runtime {
        program,
        state: Arc::clone(&state),
        output_muted: false,
    };
    let entry_succeeded = match runtime.evaluate_module_values() {
        Ok(()) => runtime.call_entry(arguments).is_ok(),
        Err(_) => false,
    };
    // Join every detached thread before deciding the exit code, so a panic or
    // failure in a thread that outlived the entry still counts.
    state.join_all_threads();
    if state.interrupted() != 0 {
        130
    } else if state.panicked() || !entry_succeeded {
        1
    } else {
        0
    }
}

/// The args record: the first word is `cmd` and the second is `flag`, both
/// verbatim; an absent word reads `""`. A third word is a usage error.
pub(super) fn build_arguments_record(words: &[String]) -> Result<Record, &'static str> {
    if words.len() > 2 {
        return Err("expected one command and one flag");
    }
    Ok(Record::from_pairs([
        ("cmd".to_owned(), words.first().cloned().unwrap_or_default()),
        ("flag".to_owned(), words.get(1).cloned().unwrap_or_default()),
    ]))
}

/// The interpreter over one program. Detached threads borrow the shared
/// program and state through `Arc`, so every field is safe to share.
pub(crate) struct Runtime {
    pub(super) program: Arc<Program>,
    pub(super) state: Arc<RuntimeState>,
    /// Whether this thread's output is muted. The entry thread owns the
    /// terminal and is not muted; a thread started by `async` is muted, so its
    /// commands write nothing to stdout or stderr.
    pub(super) output_muted: bool,
}

impl Runtime {
    /// Call the entry, passing the args record when the entry takes one.
    fn call_entry(&self, arguments: Record) -> Result<Value, Panic> {
        let entry = self.program.entry;
        let declaration = self.program.declaration(entry);
        let DeclarationKind::Function(_) = &declaration.kind else {
            return Err(self.fail());
        };
        let arguments = match declaration.parameters.as_slice() {
            [] => Vec::new(),
            [parameter] => {
                if self.program.declaration(*parameter).derived.kind != Some(Kind::Record) {
                    return Err(self.fail());
                }
                vec![Value::Record(arguments)]
            }
            _ => return Err(self.fail()),
        };
        self.call_declaration(entry, arguments)
    }

    /// Call a function with already evaluated arguments.
    pub(super) fn call_declaration(
        &self,
        declaration: DeclarationId,
        arguments: Vec<Value>,
    ) -> Result<Value, Panic> {
        let node = self.program.declaration(declaration);
        let DeclarationKind::Function(function) = &node.kind else {
            return Err(self.fail());
        };
        if node.parameters.len() != arguments.len() {
            return Err(self.fail());
        }
        let mut env = Environment::new();
        for (parameter, argument) in node.parameters.iter().zip(arguments) {
            env.insert(*parameter, argument);
        }
        self.exec_body(&function.body, &mut env)
    }

    /// Evaluate every top-level initializer once, in declaration order, on
    /// this machine, and store the result in the program's value table.
    pub(super) fn evaluate_module_values(&self) -> Result<(), Panic> {
        for (index, declaration) in self.program.declarations.iter().enumerate() {
            let Some(expression) = declaration.initializer() else {
                continue;
            };
            let env = Environment::new();
            let value = self.eval(expression, &env)?;
            if declaration.derived.kind != Some(Kind::of(&value)) {
                return Err(self.fail());
            }
            self.program
                .values
                .write_unpoisoned()
                .insert(DeclarationId(index), value);
        }
        Ok(())
    }

    /// Run a body: its statements, then its defers in reverse registration
    /// order. The stop request is suspended while defers run and re-suspended
    /// after each one that fails, so every defer gets to run its cleanup.
    fn exec_body(&self, statements: &[Statement], env: &mut Environment) -> Result<Value, Panic> {
        let mut defers: Vec<&[Statement]> = Vec::new();
        let flow = self.exec_statements(statements, env, &mut defers);

        let mut defer_result: Result<(), Panic> = Ok(());
        if !defers.is_empty() {
            while let Some(body) = defers.pop() {
                self.state.begin_cleanup();
                if let Err(failure) = self.exec_body(body, env) {
                    defer_result = Err(failure);
                }
            }
            self.state.end_cleanup();
        }

        let value = flow?;
        defer_result?;
        Ok(value.unwrap_or(Value::Nothing))
    }

    /// A runtime handle that shares the program and the run state, for one
    /// detached thread.
    pub(super) fn detached(&self) -> Runtime {
        Runtime {
            program: Arc::clone(&self.program),
            state: Arc::clone(&self.state),
            output_muted: true,
        }
    }

    pub(super) fn eval_text(
        &self,
        expression: &Expression,
        env: &Environment,
    ) -> Result<String, Panic> {
        let value = self.eval(expression, env)?;
        self.text_value(value)
    }

    /// The text of a value, or a failure when it is not text.
    pub(super) fn text_value(&self, value: Value) -> Result<String, Panic> {
        match value {
            Value::Text(text) => Ok(text),
            Value::Record(_) | Value::Nothing => Err(self.fail()),
        }
    }

    /// The failure that unwinds a body when the checked model is violated.
    /// It only returns the panic; it never signals another body's commands.
    pub(super) fn fail(&self) -> Panic {
        Panic
    }
}

#[cfg(test)]
impl Runtime {
    /// Build a runtime over a checked program and evaluate its module values,
    /// exactly as the compiled binary does at startup.
    pub(crate) fn for_testing(program: Program) -> Self {
        let runtime = Self {
            program: Arc::new(program),
            state: Arc::new(RuntimeState::new()),
            output_muted: false,
        };
        runtime
            .evaluate_module_values()
            .expect("module values evaluate at startup");
        runtime
    }

    /// Call a root-namespace function with evaluated arguments.
    pub(crate) fn call_root(&self, name: &str, arguments: Vec<Value>) -> Result<Value, Panic> {
        let declaration = self
            .program
            .namespace(self.program.root())
            .function(name)
            .ok_or_else(|| self.fail())?;
        self.call_declaration(declaration, arguments)
    }
}
