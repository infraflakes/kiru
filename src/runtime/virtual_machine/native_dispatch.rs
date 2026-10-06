//! Dispatching a native call to its handler.
//!
//! The match is exhaustive over `Native`, so a native without a handler does
//! not compile. Each handler pops its arguments off the evaluated list, runs
//! the native kernel, and answers a value or a reason the VM reports under the
//! native's own name.

use crate::model::{Record, Value};
use crate::native_registry::{Native, native_row};
use crate::runtime::Panic;
use crate::runtime::natives::{
    environment, file_system, list_operations, parse_whole_number, path_operations,
    terminal_output, text_operations,
};
use crate::runtime::processes as process;

use super::Runtime;

impl Runtime {
    /// Run one native with already evaluated arguments.
    pub(super) fn call_native(
        &self,
        native: Native,
        mut values: Vec<Value>,
    ) -> Result<Value, Panic> {
        if values.len() != crate::native_registry::native_arity(native) {
            return Err(self.fail());
        }
        match native {
            Native::Spawn => {
                let argv = self.pop_list(&mut values)?;
                let spec = self.pop_record(&mut values)?;
                let pid = process::spawn(&self.state, &spec, &argv)
                    .map_err(|reason| self.native_failure(native, reason))?;
                Ok(Value::Text(pid.to_string()))
            }
            Native::WaitFor => {
                let pid = self.pop_text(&mut values)?;
                let pid = parse_pid(&pid).map_err(|reason| self.native_failure(native, reason))?;
                match process::wait_for(&self.state, pid) {
                    Ok(code) => Ok(Value::Text(code.to_string())),
                    Err(process::WaitFailure::Reported(reason)) => {
                        Err(self.native_failure(native, reason))
                    }
                    Err(process::WaitFailure::Cancelled) => Err(self.fail()),
                }
            }
            Native::Signal => {
                let name = self.pop_text(&mut values)?;
                let pid = self.pop_text(&mut values)?;
                let pid = parse_pid(&pid).map_err(|reason| self.native_failure(native, reason))?;
                process::signal(pid, &name)
                    .map_err(|reason| self.native_failure(native, reason))?;
                Ok(Value::Nothing)
            }
            Native::Write => {
                let text = self.pop_text(&mut values)?;
                let file_descriptor = self.pop_text(&mut values)?;
                terminal_output::write(&file_descriptor, &text)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::IsTerminal => {
                let file_descriptor = self.pop_text(&mut values)?;
                terminal_output::is_terminal(&file_descriptor)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::ReadStdin => {
                terminal_output::read_stdin().map_err(|reason| self.native_failure(native, reason))
            }
            Native::Read => {
                let path = self.pop_text(&mut values)?;
                file_system::read(&path).map_err(|reason| self.native_failure(native, reason))
            }
            Native::WriteFile => {
                let text = self.pop_text(&mut values)?;
                let path = self.pop_text(&mut values)?;
                file_system::write(&path, &text)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::Append => {
                let text = self.pop_text(&mut values)?;
                let path = self.pop_text(&mut values)?;
                file_system::append(&path, &text)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::Remove => {
                let path = self.pop_text(&mut values)?;
                file_system::remove(&path).map_err(|reason| self.native_failure(native, reason))
            }
            Native::Exists => {
                let path = self.pop_text(&mut values)?;
                Ok(file_system::exists(&path))
            }
            Native::TempPath => {
                file_system::temp_file().map_err(|reason| self.native_failure(native, reason))
            }
            Native::Glob => {
                let pattern = self.pop_text(&mut values)?;
                file_system::glob(&pattern).map_err(|reason| self.native_failure(native, reason))
            }
            Native::Length => {
                let text = self.pop_text(&mut values)?;
                Ok(text_operations::length(&text))
            }
            Native::Slice => {
                let end = self.pop_text(&mut values)?;
                let start = self.pop_text(&mut values)?;
                let text = self.pop_text(&mut values)?;
                text_operations::slice(&text, &start, &end)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::Find => {
                let needle = self.pop_text(&mut values)?;
                let text = self.pop_text(&mut values)?;
                Ok(text_operations::find(&text, &needle))
            }
            Native::Trim => {
                let text = self.pop_text(&mut values)?;
                Ok(text_operations::trim(&text))
            }
            Native::Split => {
                let separator = self.pop_text(&mut values)?;
                let text = self.pop_text(&mut values)?;
                Ok(text_operations::split(&text, &separator))
            }
            Native::Lines => {
                let text = self.pop_text(&mut values)?;
                Ok(text_operations::lines(&text))
            }
            Native::ListLength => {
                let list = self.pop_list(&mut values)?;
                Ok(list_operations::len(&list))
            }
            Native::ListGet => {
                let index = self.pop_text(&mut values)?;
                let list = self.pop_list(&mut values)?;
                Ok(list_operations::get(&list, &index))
            }
            Native::ListAppend => {
                let text = self.pop_text(&mut values)?;
                let list = self.pop_list(&mut values)?;
                Ok(list_operations::append(&list, &text))
            }
            Native::ListReverse => {
                let list = self.pop_list(&mut values)?;
                Ok(list_operations::reverse(&list))
            }
            Native::ListSort => {
                let list = self.pop_list(&mut values)?;
                Ok(list_operations::sort(&list))
            }
            Native::EnvVar => {
                let name = self.pop_text(&mut values)?;
                Ok(environment::var(&name))
            }
            Native::CurrentDir => {
                environment::current_dir().map_err(|reason| self.native_failure(native, reason))
            }
            Native::PathBase => {
                let path = self.pop_text(&mut values)?;
                Ok(path_operations::base(&path))
            }
            Native::PathDir => {
                let path = self.pop_text(&mut values)?;
                Ok(path_operations::dir(&path))
            }
            Native::PathExt => {
                let path = self.pop_text(&mut values)?;
                Ok(path_operations::ext(&path))
            }
            Native::Sleep => {
                let seconds = self.pop_text(&mut values)?;
                let seconds = parse_whole_number(&seconds)
                    .map_err(|reason| self.native_failure(native, reason))?;
                process::sleep_seconds(&self.state, seconds as u64)?;
                Ok(Value::Nothing)
            }
        }
    }

    /// Report a native's failure under the native's own name and unwind the
    /// body, exactly as a failed command does.
    fn native_failure(&self, native: Native, reason: String) -> Panic {
        let row = native_row(native);
        let qualified = format!("{}::{}", row.path.join("::"), row.name);
        process::failure(&self.state, &qualified, &reason)
    }

    /// Pop the last evaluated argument and require it to be text.
    fn pop_text(&self, values: &mut Vec<Value>) -> Result<String, Panic> {
        let Some(value) = values.pop() else {
            return Err(self.fail());
        };
        self.text_value(value)
    }

    /// Pop the last evaluated argument and require it to be a list.
    fn pop_list(&self, values: &mut Vec<Value>) -> Result<Vec<String>, Panic> {
        let Some(value) = values.pop() else {
            return Err(self.fail());
        };
        match value {
            Value::List(list) => Ok(list),
            _ => Err(self.fail()),
        }
    }

    /// Pop the last evaluated argument and require it to be a record.
    fn pop_record(&self, values: &mut Vec<Value>) -> Result<Record, Panic> {
        let Some(value) = values.pop() else {
            return Err(self.fail());
        };
        match value {
            Value::Record(record) => Ok(record),
            _ => Err(self.fail()),
        }
    }
}

/// Parse a process id written in text. A pid is positive.
fn parse_pid(text: &str) -> Result<i32, String> {
    let pid = text
        .parse::<i32>()
        .map_err(|_| format!("`{text}` is not a process id"))?;
    if pid <= 0 {
        return Err(format!("`{text}` is not a process id"));
    }
    Ok(pid)
}
