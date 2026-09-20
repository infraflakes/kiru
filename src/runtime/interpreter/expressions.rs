//! Expression evaluation: text, records, references, calls, fields, methods.

use crate::compiler::{
    Command, DeclarationId, DeclarationKind, Expression, Field, Native, Record, Value,
};
use crate::runtime::Panic;

use super::{Environment, Runtime};

impl Runtime {
    /// Evaluate an expression against one body's environment.
    pub(super) fn eval(&self, expression: &Expression, env: &Environment) -> Result<Value, Panic> {
        match expression {
            Expression::Text { value, .. } => Ok(Value::Text(value.clone())),
            Expression::Record { fields, .. } => Ok(Value::Record(self.eval_fields(fields, env)?)),
            Expression::Reference { declaration, .. } => {
                if let Some(value) = env.get(declaration) {
                    return Ok(value.clone());
                }
                self.program
                    .values
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(declaration)
                    .cloned()
                    .ok_or_else(|| self.fail())
            }
            Expression::Call {
                callee, arguments, ..
            } => self.eval_call(*callee, arguments, env),
            Expression::Field { target, name, .. } => {
                let Value::Record(record) = self.eval(target, env)? else {
                    return Err(self.fail());
                };
                Ok(Value::Text(record.get(name).to_owned()))
            }
            Expression::Method {
                target,
                method,
                arguments,
                ..
            } => self.eval_method(target, *method, arguments, env),
            Expression::Add { left, right, .. } => {
                let left = self.eval_text(left, env)?;
                let right = self.eval_text(right, env)?;
                Ok(Value::Text(left + &right))
            }
        }
    }

    fn eval_call(
        &self,
        callee: DeclarationId,
        arguments: &[Expression],
        env: &Environment,
    ) -> Result<Value, Panic> {
        // `async` takes an invocation rather than a value, so its argument
        // must not be evaluated here: evaluating it would run the call on the
        // spawning thread instead of the new one.
        if matches!(
            self.program.declaration(callee).kind,
            DeclarationKind::Native(Native::Async)
        ) {
            return self.spawn_thread(arguments, env);
        }
        let values = self.eval_arguments(arguments, env)?;
        self.invoke_call(callee, values)
    }

    /// Run a function or a native with already evaluated arguments. The spawn
    /// site and a detached thread both call this, so a thread never evaluates
    /// an expression against a foreign environment.
    pub(super) fn invoke_call(
        &self,
        callee: DeclarationId,
        mut values: Vec<Value>,
    ) -> Result<Value, Panic> {
        match &self.program.declaration(callee).kind {
            DeclarationKind::Function(_) => self.call_declaration(callee, values),
            DeclarationKind::Native(Native::Command) => {
                let Some(value) = values.pop() else {
                    return Err(self.fail());
                };
                Ok(Value::Command(Command::new(self.text_value(value)?)))
            }
            DeclarationKind::Native(Native::Wait) => {
                self.state.join_children(std::thread::current().id());
                Ok(Value::Nothing)
            }
            DeclarationKind::Native(Native::Panic) => {
                self.state.panic();
                Err(Panic)
            }
            _ => Err(self.fail()),
        }
    }

    /// Start an invocation on its own OS thread. The invocation's receiver
    /// and arguments are evaluated here, at the spawn site; the thread then
    /// runs the call or method with those values. `std::async` yields nothing.
    fn spawn_thread(&self, arguments: &[Expression], env: &Environment) -> Result<Value, Panic> {
        let Some(invocation) = arguments.first() else {
            return Err(self.fail());
        };
        match invocation {
            Expression::Call {
                callee,
                arguments: call_arguments,
                ..
            } => {
                let values = self.eval_arguments(call_arguments, env)?;
                let callee = *callee;
                let runtime = self.detached();
                self.state.spawn_thread(move || {
                    if let Ok(value) = runtime.invoke_call(callee, values) {
                        let _ = runtime.execute_produced_value(value);
                    }
                });
                Ok(Value::Nothing)
            }
            Expression::Method {
                target,
                method,
                arguments: method_arguments,
                ..
            } => {
                let target = self.eval(target, env)?;
                let values = self.eval_arguments(method_arguments, env)?;
                let method = *method;
                let runtime = self.detached();
                self.state.spawn_thread(move || {
                    if let Ok(value) = runtime.invoke_method(target, method, values) {
                        let _ = runtime.execute_produced_value(value);
                    }
                });
                Ok(Value::Nothing)
            }
            _ => Err(self.fail()),
        }
    }

    pub(super) fn eval_fields(&self, fields: &[Field], env: &Environment) -> Result<Record, Panic> {
        let mut record = Record::new();
        for field in fields {
            let text = self.eval_text(&field.value, env)?;
            record.set(field.name.clone(), text);
        }
        Ok(record)
    }
}
