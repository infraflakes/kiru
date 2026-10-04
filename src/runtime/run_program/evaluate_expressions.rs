//! Expression evaluation: text, records, references, calls, and fields.

use crate::compiler::{
    DeclarationId, DeclarationKind, Expression, Field, Native, Record, RwLockExt, Value,
};
use crate::runtime::Panic;
use crate::runtime::manage_processes as process;

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
                    .read_unpoisoned()
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
        let values = self.eval_arguments(arguments, env)?;
        self.invoke_call(callee, values)
    }

    /// Evaluate every argument expression in order.
    pub(super) fn eval_arguments(
        &self,
        arguments: &[Expression],
        env: &Environment,
    ) -> Result<Vec<Value>, Panic> {
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(self.eval(argument, env)?);
        }
        Ok(values)
    }

    /// Spawn one call on its own OS thread. The call's arguments are evaluated
    /// here, at the spawn site; the thread then runs the call with those
    /// values. The `async` statement calls this.
    pub(super) fn spawn_call(&self, call: &Expression, env: &Environment) -> Result<(), Panic> {
        let Expression::Call {
            callee, arguments, ..
        } = call
        else {
            return Err(self.fail());
        };
        let values = self.eval_arguments(arguments, env)?;
        let callee = *callee;
        let runtime = self.detached();
        self.state.spawn_thread(move || {
            let _ = runtime.invoke_call(callee, values);
        });
        Ok(())
    }

    /// Run a function or a native with already evaluated arguments. The spawn
    /// site and a detached thread both call this, so a thread never evaluates
    /// an expression against a foreign environment.
    pub(super) fn invoke_call(
        &self,
        callee: DeclarationId,
        values: Vec<Value>,
    ) -> Result<Value, Panic> {
        match &self.program.declaration(callee).kind {
            DeclarationKind::Function(_) => self.call_declaration(callee, values),
            DeclarationKind::Native(native) => self.call_native(*native, values),
            DeclarationKind::Text(_) | DeclarationKind::Record(_) | DeclarationKind::Binding(_) => {
                Err(self.fail())
            }
        }
    }

    /// Run one native with already evaluated arguments. The match is
    /// exhaustive over `Native`, so a native without a handler does not
    /// compile.
    fn call_native(&self, native: Native, mut values: Vec<Value>) -> Result<Value, Panic> {
        debug_assert_eq!(
            values.len(),
            crate::compiler::native_arity(native),
            "a native handler consumes exactly its declared parameters"
        );
        match native {
            Native::Run => {
                let line = self.pop_text(&mut values)?;
                let code = process::run(&self.state, &line, self.output_muted)?;
                Ok(Value::Text(code.to_string()))
            }
            Native::Quote => {
                let text = self.pop_text(&mut values)?;
                Ok(Value::Text(process::quote_shell_word(&text)))
            }
        }
    }

    /// Pop the last evaluated argument and require it to be text.
    fn pop_text(&self, values: &mut Vec<Value>) -> Result<String, Panic> {
        let Some(value) = values.pop() else {
            return Err(self.fail());
        };
        self.text_value(value)
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
