//! Statement execution: binds, assignments, returns, switches, and defers.

use crate::compiler::{Statement, Value};
use crate::runtime::Panic;
use crate::runtime::process;

use super::{Environment, Runtime};

impl Runtime {
    /// Run the value a call or method chain produced the way a bare statement
    /// does: a command runs, forwarding stdout only when `.stream()` is set,
    /// and any other value is discarded. A detached thread runs its produced
    /// value through this, so an async start executes like a statement.
    pub(super) fn execute_produced_value(&self, value: Value) -> Result<(), Panic> {
        if let Value::Command(command) = value {
            process::run(
                &self.state,
                &command,
                false,
                command.stream,
                self.default_shell.as_deref(),
            )?;
        }
        Ok(())
    }

    /// Run the statements of one body. Nested blocks share the environment
    /// and the defer list, so a switch arm assigns the enclosing bindings.
    pub(super) fn exec_statements<'statement>(
        &self,
        statements: &'statement [Statement],
        env: &mut Environment,
        defers: &mut Vec<&'statement [Statement]>,
    ) -> Result<Option<Value>, Panic> {
        for statement in statements {
            if self.state.cancelled() {
                return Err(self.fail());
            }
            match statement {
                Statement::Bind {
                    declaration, value, ..
                }
                | Statement::Assign {
                    declaration, value, ..
                } => {
                    let value = self.eval(value, env)?;
                    env.insert(*declaration, value);
                }
                Statement::Expression(expression) => {
                    self.execute_produced_value(self.eval(expression, env)?)?;
                }
                Statement::Return { value, .. } => {
                    return Ok(Some(self.eval(value, env)?));
                }
                Statement::Switch {
                    subject,
                    cases,
                    default,
                    ..
                } => {
                    let subject = self.eval_text(subject, env)?;
                    let mut matched = false;
                    for case in cases {
                        if self.eval_text(&case.pattern, env)? == subject {
                            if let Some(value) = self.exec_statements(&case.body, env, defers)? {
                                return Ok(Some(value));
                            }
                            matched = true;
                            break;
                        }
                    }
                    if !matched
                        && let Some(default) = default
                        && let Some(value) = self.exec_statements(default, env, defers)?
                    {
                        return Ok(Some(value));
                    }
                }
                Statement::Defer { body, .. } => defers.push(body),
            }
        }
        Ok(None)
    }
}
