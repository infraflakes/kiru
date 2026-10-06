//! Statement execution: binds, assignments, returns, panic, switches, and
//! defers.

use crate::compiler::{Statement, Value};
use crate::runtime::Panic;

use super::{Environment, Runtime};

impl Runtime {
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
                Statement::FieldAssign {
                    declaration,
                    field,
                    value,
                    ..
                } => {
                    let text = self.eval_text(value, env)?;
                    match env.get_mut(declaration) {
                        Some(Value::Record(record)) => record.set(field.clone(), text),
                        _ => return Err(self.fail()),
                    }
                }
                Statement::Expression(expression) => {
                    // A bare call runs and its result is discarded.
                    let _ = self.eval(expression, env)?;
                }
                Statement::Return { value, .. } => {
                    return Ok(Some(match value {
                        Some(value) => self.eval(value, env)?,
                        None => Value::Nothing,
                    }));
                }
                Statement::Panic { .. } => {
                    self.state.panic();
                    return Err(Panic);
                }
                Statement::Async { call, .. } => {
                    self.spawn_call(call, env)?;
                }
                Statement::Wait { .. } => {
                    self.state.join_children(std::thread::current().id());
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
