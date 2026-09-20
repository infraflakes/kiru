//! Command chain methods: builders that configure a command and terminals
//! that run it.

use crate::compiler::{Expression, Method, Value};
use crate::runtime::Panic;
use crate::runtime::process;

use super::{Environment, Runtime};

impl Runtime {
    pub(super) fn eval_method(
        &self,
        target: &Expression,
        method: Method,
        arguments: &[Expression],
        env: &Environment,
    ) -> Result<Value, Panic> {
        let target = self.eval(target, env)?;
        let values = self.eval_arguments(arguments, env)?;
        self.invoke_method(target, method, values)
    }

    /// Apply a method to an already evaluated target. The spawn site and a
    /// detached thread both call this, so a thread never touches an
    /// environment. A terminal (`.out()` or `.code()`) runs the command; a
    /// builder returns a new command value.
    pub(super) fn invoke_method(
        &self,
        target: Value,
        method: Method,
        mut values: Vec<Value>,
    ) -> Result<Value, Panic> {
        match method {
            Method::In => {
                let Value::Command(mut command) = target else {
                    return Err(self.fail());
                };
                let Some(value) = values.pop() else {
                    return Err(self.fail());
                };
                command.dir = Some(self.text_value(value)?);
                Ok(Value::Command(command))
            }
            Method::Direnv => {
                let Value::Command(mut command) = target else {
                    return Err(self.fail());
                };
                command.direnv = true;
                Ok(Value::Command(command))
            }
            Method::Env => {
                let Value::Command(mut command) = target else {
                    return Err(self.fail());
                };
                let Some(Value::Record(record)) = values.pop() else {
                    return Err(self.fail());
                };
                command.merge_env(&record);
                Ok(Value::Command(command))
            }
            Method::Shell => {
                let Value::Command(mut command) = target else {
                    return Err(self.fail());
                };
                let Some(value) = values.pop() else {
                    return Err(self.fail());
                };
                command.shell = Some(self.text_value(value)?);
                Ok(Value::Command(command))
            }
            Method::Timeout => {
                let Value::Command(mut command) = target else {
                    return Err(self.fail());
                };
                let Some(value) = values.pop() else {
                    return Err(self.fail());
                };
                let text = self.text_value(value)?;
                let seconds = match text.parse::<u64>() {
                    Ok(seconds) => seconds,
                    Err(_) => {
                        eprintln!("kiru: {}: timeout must be whole seconds", command.line);
                        self.state.record_failure();
                        return Err(self.fail());
                    }
                };
                command.timeout = Some(seconds);
                Ok(Value::Command(command))
            }
            Method::Stream => {
                let Value::Command(mut command) = target else {
                    return Err(self.fail());
                };
                command.stream = true;
                Ok(Value::Command(command))
            }
            Method::Out | Method::Code => {
                let Value::Command(command) = target else {
                    return Err(self.fail());
                };
                let capture = method == Method::Out;
                let outcome = process::run(
                    &self.state,
                    &command,
                    capture,
                    command.stream,
                    self.default_shell.as_deref(),
                )?;
                Ok(Value::Text(if capture {
                    outcome.output
                } else {
                    outcome.code.to_string()
                }))
            }
        }
    }
}
