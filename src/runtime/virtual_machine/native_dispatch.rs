//! Dispatching a native call to its handler.
//!
//! The match is exhaustive over `Native`, so a native without a handler does
//! not compile. One decoder, driven by the parameter shapes the row declared,
//! checks the arity and every argument's shape before the handler runs; a
//! handler receives typed arguments in declaration order and only calls the
//! kernel. The failure it reports names the native by its row's qualified
//! path.

use std::sync::Arc;

use crate::compiler::model::RuntimeShape;
use crate::compiler::native_registry::{Native, NativeArgumentKind, native_shape};
use crate::runtime::Panic;
use crate::runtime::natives::{environment, file_system, terminal_output, text_operations};
use crate::runtime::processes as process;
use crate::runtime::{Text, Value};

use super::Runtime;

/// One native call's arguments, already checked against the row's declared
/// parameter shapes and kept in declaration order.
struct NativeArguments {
    values: Vec<Value>,
}

impl NativeArguments {
    /// The argument at a declared position, as any value.
    fn value(&self, index: usize) -> Value {
        self.values[index].clone()
    }

    /// The text argument at a declared position.
    fn text(&self, index: usize) -> &Text {
        match self.values[index].text() {
            Some(text) => text,
            None => unreachable!("the decoder checked the row's declared kind"),
        }
    }

    /// The integer argument at a declared position.
    fn int(&self, index: usize) -> i64 {
        match self.values[index].integer() {
            Some(number) => number,
            None => unreachable!("the decoder checked the row's declared kind"),
        }
    }

    /// The list argument at a declared position, keeping the value shared.
    fn list(&self, index: usize) -> Arc<Vec<Value>> {
        match self.values[index].list() {
            Some(values) => Arc::clone(values),
            None => unreachable!("the decoder checked the row's declared kind"),
        }
    }
}

impl Runtime {
    /// Run one native with already evaluated arguments. The row declares the
    /// order and the type of every parameter, so the decoder checks the call
    /// once and the handler reads each argument by that position.
    pub(super) fn call_native(&self, native: Native, values: Vec<Value>) -> Result<Value, Panic> {
        let arguments = self.decode_arguments(native, values)?;
        match native {
            Native::VecNew => Ok(Value::Vec(Arc::new(Vec::new()))),
            Native::VecPush => {
                let mut vec = arguments.list(0);
                let item = arguments.value(1);
                // Unshare before writing, so the copy a clone made is real.
                Arc::make_mut(&mut vec).push(item);
                Ok(Value::Vec(vec))
            }
            Native::VecReplace => {
                let mut vec = arguments.list(0);
                let index = arguments.int(1);
                let item = arguments.value(2);
                let length = vec.len();
                if index >= 0 && (index as usize) < length {
                    // Unshare before writing, so the copy a clone made is real.
                    Arc::make_mut(&mut vec)[index as usize] = item;
                } else {
                    return Err(self.native_failure(
                        native,
                        format!("index {index} is outside a Vec of {length} element(s)"),
                    ));
                }
                Ok(Value::Vec(vec))
            }
            Native::VecAt => {
                let vec = arguments.list(0);
                let index = arguments.int(1);
                match vec.get(index as usize) {
                    Some(element) => Ok(element.clone()),
                    None => Err(self.native_failure(
                        native,
                        format!("index {index} is outside a Vec of {} element(s)", vec.len()),
                    )),
                }
            }
            Native::VecLen => {
                let vec = arguments.list(0);
                Ok(Value::Int(vec.len() as i64))
            }
            Native::PosixSpawn => {
                let argv = arguments.list(0);
                let argv = argv
                    .iter()
                    .map(|value| self.text_value(value))
                    .collect::<Result<Vec<String>, Panic>>()?;
                let pid = process::spawn(&self.state, &argv)
                    .map_err(|reason| self.native_failure(native, reason))?;
                Ok(Value::Int(pid as i64))
            }
            Native::Waitpid => {
                let pid = arguments.int(0);
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
            Native::Killpg => {
                let pid = arguments.int(0);
                let number = arguments.int(1);
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
                let file_descriptor = arguments.int(0);
                let text = arguments.text(1);
                terminal_output::write(file_descriptor, text)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::Isatty => {
                let file_descriptor = arguments.int(0);
                terminal_output::is_terminal(file_descriptor)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::ReadStdin => {
                terminal_output::read_stdin().map_err(|reason| self.native_failure(native, reason))
            }
            Native::Read => {
                let path = arguments.text(0);
                file_system::read(path).map_err(|reason| self.native_failure(native, reason))
            }
            Native::WriteFile => {
                let path = arguments.text(0);
                let text = arguments.text(1);
                file_system::write(path, text).map_err(|reason| self.native_failure(native, reason))
            }
            Native::Unlink => {
                let path = arguments.text(0);
                file_system::remove(path).map_err(|reason| self.native_failure(native, reason))
            }
            Native::Access => {
                let path = arguments.text(0);
                Ok(file_system::exists(path))
            }
            Native::Mkstemp => {
                let template = arguments.text(0);
                file_system::mkstemp(template).map_err(|reason| self.native_failure(native, reason))
            }
            Native::Readdir => {
                let path = arguments.text(0);
                file_system::list(path).map_err(|reason| self.native_failure(native, reason))
            }
            Native::Strlen => {
                let text = arguments.text(0);
                Ok(Value::Int(text.byte_len() as i64))
            }
            Native::Memchr => {
                let text = arguments.text(0);
                let needle = arguments.text(1);
                Ok(text_operations::find(text, needle))
            }
            Native::ByteAt => {
                let text = arguments.text(0);
                let index = arguments.int(1);
                text_operations::byte(text, index)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::Substr => {
                let text = arguments.text(0);
                let start = arguments.int(1);
                let end = arguments.int(2);
                text_operations::slice(text, start, end)
                    .map_err(|reason| self.native_failure(native, reason))
            }
            Native::Getenv => {
                let name = arguments.text(0);
                Ok(environment::var(name))
            }
            Native::Getcwd => {
                environment::current_dir().map_err(|reason| self.native_failure(native, reason))
            }
            Native::Nanosleep => {
                let seconds = arguments.int(0);
                let seconds = u64::try_from(seconds)
                    .map_err(|_| self.native_failure(native, "seconds out of range".to_owned()))?;
                process::sleep_seconds(&self.state, seconds)?;
                Ok(Value::Nothing)
            }
        }
    }

    /// Decode a native's evaluated arguments from its row: the arity and every
    /// argument's shape are checked here, once, before a handler runs.
    fn decode_arguments(
        &self,
        native: Native,
        values: Vec<Value>,
    ) -> Result<NativeArguments, Panic> {
        let Some(signature) = self
            .bytecode
            .natives
            .iter()
            .find(|signature| signature.native == native)
        else {
            return Err(self.fail());
        };
        if values.len() != signature.parameters.len() {
            return Err(self.fail());
        }
        for (value, kind) in values.iter().zip(&signature.parameters) {
            if !has_kind(*kind, value) {
                return Err(self.fail());
            }
        }
        Ok(NativeArguments { values })
    }

    /// Report a native's failure under the native's own qualified name, as a
    /// call, and unwind the body, exactly as a failed command does.
    fn native_failure(&self, native: Native, reason: String) -> Panic {
        let row = native_shape(native);
        let qualified = format!("{}::{}", row.path.join("::"), row.name);
        process::failure(&self.state, &format!("{qualified}(...)"), &reason)
    }
}

/// Whether a runtime value has the shape the row declared for its position,
/// read from the value's declared runtime shape: the row names a shape of the
/// same model, so the decoder checks the one fact and never a kind of its own.
fn has_kind(kind: NativeArgumentKind, value: &Value) -> bool {
    let shape = value.runtime_shape();
    match kind {
        NativeArgumentKind::Text => shape == RuntimeShape::Text,
        NativeArgumentKind::Int => shape == RuntimeShape::Int,
        NativeArgumentKind::Bool => shape == RuntimeShape::Bool,
        NativeArgumentKind::List => shape == RuntimeShape::List,
        NativeArgumentKind::Object => {
            shape == RuntimeShape::Record || shape == RuntimeShape::Variant
        }
        NativeArgumentKind::Function => shape == RuntimeShape::Function,
        // A type parameter is the caller's type: every shape but the absence
        // fits.
        NativeArgumentKind::Any => value.carries(),
    }
}
