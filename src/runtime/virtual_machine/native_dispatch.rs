//! Dispatching a native call to its handler.
//!
//! The match is exhaustive over `Native`, so a native without a handler does
//! not compile. Each handler pops its arguments off the evaluated Vec, runs
//! the native kernel, and answers a value or a reason the VM reports under the
//! native's own name.

use std::sync::Arc;

use crate::native_registry::{Native, native_shape};
use crate::runtime::Panic;
use crate::runtime::natives::{environment, file_system, terminal_output, text_operations};
use crate::runtime::processes as process;
use crate::runtime::{Text, Value};

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
            Native::VecNew => Ok(Value::Vec(Arc::new(Vec::new()))),
            Native::VecPush => {
                let Some(item) = values.pop() else {
                    return Err(self.fail());
                };
                let mut vec = self.pop_vec(&mut values)?;
                // Unshare before writing, so the copy a clone made is real.
                Arc::make_mut(&mut vec).push(item);
                Ok(Value::Vec(vec))
            }
            Native::VecReplace => {
                let Some(item) = values.pop() else {
                    return Err(self.fail());
                };
                let index = self.pop_int(&mut values)?;
                let mut vec = self.pop_vec(&mut values)?;
                let length = vec.len();
                if index >= 0 && (index as usize) < length {
                    // Unshare before writing, so the copy a clone made is real.
                    Arc::make_mut(&mut vec)[index as usize] = item;
                } else {
                    return Err(self.index_failure("xs.replace(index, item)", index, length));
                }
                Ok(Value::Vec(vec))
            }
            Native::VecAt => {
                let index = self.pop_int(&mut values)?;
                let vec = self.pop_vec(&mut values)?;
                let Some(element) = vec.get(index as usize) else {
                    return Err(self.index_failure("xs.at(index)", index, vec.len()));
                };
                Ok(element.clone())
            }
            Native::VecLen => {
                let vec = self.pop_vec(&mut values)?;
                Ok(Value::Int(vec.len() as i64))
            }
            Native::Spawn => {
                let argv = self.pop_list(&mut values)?;
                let argv = argv
                    .into_iter()
                    .map(|value| self.text_value(value))
                    .collect::<Result<Vec<String>, Panic>>()?;
                let pid = process::spawn(&self.state, &argv)
                    .map_err(|reason| self.native_failure(native, reason))?;
                Ok(Value::Int(pid as i64))
            }
            Native::Reap => {
                let pid = self.pop_int(&mut values)?;
                let pid = i32::try_from(pid).map_err(|_| {
                    self.native_failure(native, "process id out of range".to_owned())
                })?;
                match process::wait_for(&self.state, pid) {
                    Ok(code) => Ok(Value::Int(code as i64)),
                    Err(process::WaitFailure::Reported(reason)) => {
                        Err(self.native_failure(native, reason))
                    }
                    Err(process::WaitFailure::Cancelled) => Err(self.fail()),
                }
            }
            Native::SignalGroup => {
                let number = self.pop_int(&mut values)?;
                let pid = self.pop_int(&mut values)?;
                let pid = i32::try_from(pid).map_err(|_| {
                    self.native_failure(native, "process id out of range".to_owned())
                })?;
                let number = i32::try_from(number).map_err(|_| {
                    self.native_failure(native, "signal number out of range".to_owned())
                })?;
                process::signal(pid, number)
                    .map_err(|reason| self.native_failure(native, reason))?;
                Ok(Value::Nothing)
            }
            Native::Write => {
                let text = self.pop_text(&mut values)?;
                let file_descriptor = self.pop_int(&mut values)?;
                terminal_output::write(file_descriptor, &text)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::IsTerminal => {
                let file_descriptor = self.pop_int(&mut values)?;
                terminal_output::is_terminal(file_descriptor)
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
            Native::Remove => {
                let path = self.pop_text(&mut values)?;
                file_system::remove(&path).map_err(|reason| self.native_failure(native, reason))
            }
            Native::Exists => {
                let path = self.pop_text(&mut values)?;
                Ok(file_system::exists(&path))
            }
            Native::Mkstemp => {
                let template = self.pop_text(&mut values)?;
                file_system::mkstemp(&template)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::List => {
                let path = self.pop_text(&mut values)?;
                file_system::list(&path).map_err(|reason| self.native_failure(native, reason))
            }
            Native::Length => {
                let text = self.pop_text(&mut values)?;
                Ok(Value::Int(text.byte_len() as i64))
            }
            Native::Find => {
                let needle = self.pop_text(&mut values)?;
                let text = self.pop_text(&mut values)?;
                Ok(text_operations::find(&text, &needle))
            }
            Native::Byte => {
                let index = self.pop_int(&mut values)?;
                let text = self.pop_text(&mut values)?;
                text_operations::byte(&text, index)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::Slice => {
                let end = self.pop_int(&mut values)?;
                let start = self.pop_int(&mut values)?;
                let text = self.pop_text(&mut values)?;
                text_operations::slice(&text, start, end)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::EnvVar => {
                let name = self.pop_text(&mut values)?;
                Ok(environment::var(&name))
            }
            Native::CurrentDir => {
                environment::current_dir().map_err(|reason| self.native_failure(native, reason))
            }
            Native::SleepSeconds => {
                let seconds = self.pop_int(&mut values)?;
                let seconds = u64::try_from(seconds)
                    .map_err(|_| self.native_failure(native, "seconds out of range".to_owned()))?;
                process::sleep_seconds(&self.state, seconds)?;
                Ok(Value::Nothing)
            }
        }
    }

    /// Report a native's failure under the native's own name, as a call, and
    /// unwind the body, exactly as a failed command does.
    fn native_failure(&self, native: Native, reason: String) -> Panic {
        let row = native_shape(native);
        let qualified = format!("{}::{}", row.path.join("::"), row.name);
        process::failure(&self.state, &format!("{qualified}(...)"), &reason)
    }

    /// Pop the last evaluated argument and require it to be text.
    fn pop_text(&self, values: &mut Vec<Value>) -> Result<Text, Panic> {
        let Some(value) = values.pop() else {
            return Err(self.fail());
        };
        match value {
            Value::Text(text) => Ok(text),
            _ => Err(self.fail()),
        }
    }

    /// Pop the last evaluated argument and require it to be a Vec.
    fn pop_list(&self, values: &mut Vec<Value>) -> Result<Vec<Value>, Panic> {
        let Some(value) = values.pop() else {
            return Err(self.fail());
        };
        match value {
            Value::Vec(vec) => Ok((*vec).clone()),
            _ => Err(self.fail()),
        }
    }

    /// Pop the last evaluated argument and require it to be a Vec, keeping the
    /// value shared. A writer unshares it before changing it, so the copy a
    /// clone made stays a copy.
    fn pop_vec(&self, values: &mut Vec<Value>) -> Result<Arc<Vec<Value>>, Panic> {
        let Some(value) = values.pop() else {
            return Err(self.fail());
        };
        match value {
            Value::Vec(vec) => Ok(vec),
            _ => Err(self.fail()),
        }
    }

    /// Pop the last evaluated argument and require it to be an integer.
    fn pop_int(&self, values: &mut Vec<Value>) -> Result<i64, Panic> {
        let Some(value) = values.pop() else {
            return Err(self.fail());
        };
        match value {
            Value::Int(number) => Ok(number),
            _ => Err(self.fail()),
        }
    }
}
