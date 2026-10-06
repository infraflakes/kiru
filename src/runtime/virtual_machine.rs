//! The bytecode virtual machine.
//!
//! A register machine over the lowered program. Module values run once at
//! startup into the globals table, then the entry runs. A function call builds
//! a fresh frame. The `async`
//! keyword starts an OS thread; `wait` joins the threads the calling thread
//! spawned. Native calls dispatch in `native_dispatch`, and the test-only entry
//! points live in `testing`.

mod native_dispatch;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

use std::sync::{Arc, RwLock};

#[cfg(test)]
use std::collections::HashMap;

use crate::bytecode::{Bytecode, Instruction};
use crate::lock_recovery::RwLockExt;
use crate::model::{Record, Value};
use crate::runtime::Panic;

use super::processes::{self as process, RuntimeState};

/// Run a program's entry with the words after the program name. Returns the
/// process exit code: zero on success, one on failure or panic, and 130 when
/// a signal arrived.
pub(crate) fn run(bytecode: Arc<Bytecode>, words: &[String]) -> i32 {
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
    let global_count = bytecode.module_values.len();
    let runtime = Runtime {
        bytecode,
        globals: Arc::new(RwLock::new(vec![Value::Nothing; global_count])),
        state: Arc::clone(&state),
        #[cfg(test)]
        symbols: HashMap::new(),
    };
    let entry_status = match runtime.evaluate_module_values() {
        Ok(()) => runtime.call_entry(arguments),
        Err(_) => Err(Panic),
    };
    // Join every detached thread before deciding the exit code, so a failure in
    // a thread that outlived the entry still counts.
    state.join_all_threads();
    if state.interrupted() != 0 {
        130
    } else if state.panicked() {
        1
    } else {
        match entry_status {
            Ok(Value::Text(status)) => status_code(&status),
            Ok(_) => 0,
            Err(_) => 1,
        }
    }
}

/// The process status a program's `main` returned: a whole number is the exit
/// code, clamped to `0..=255`; any other text is a failure message printed to
/// stderr with status `1`.
fn status_code(status: &str) -> i32 {
    match status.parse::<u32>() {
        Ok(code) => code.min(255) as i32,
        Err(_) => {
            eprintln!("{status}");
            1
        }
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

/// The virtual machine over one program. Detached threads share the bytecode,
/// the globals table, and the run state through `Arc`, so every field is safe
/// to share.
pub(crate) struct Runtime {
    pub(super) bytecode: Arc<Bytecode>,
    pub(super) globals: Arc<RwLock<Vec<Value>>>,
    pub(super) state: Arc<RuntimeState>,
    /// The root-namespace function codes, for the tests that call by name.
    #[cfg(test)]
    pub(super) symbols: HashMap<String, u32>,
}

/// One call frame: a register file.
struct Frame {
    registers: Vec<Value>,
}

impl Runtime {
    /// Evaluate every module value once, in declaration order, and store the
    /// result in the globals table.
    fn evaluate_module_values(&self) -> Result<(), Panic> {
        for (index, code) in self.bytecode.module_values.iter().enumerate() {
            let value = self.run_code(*code, Vec::new())?;
            self.globals.write_unpoisoned()[index] = value;
        }
        Ok(())
    }

    /// Call the entry, passing the args record when the entry takes one.
    fn call_entry(&self, arguments: Record) -> Result<Value, Panic> {
        let arity = self.bytecode.codes[self.bytecode.entry as usize].arity;
        let arguments = match arity {
            0 => Vec::new(),
            1 => vec![Value::Record(arguments)],
            _ => return Err(self.fail()),
        };
        self.run_code(self.bytecode.entry, arguments)
    }

    /// Run one code with a fresh frame and already evaluated arguments.
    fn run_code(&self, code_id: u32, arguments: Vec<Value>) -> Result<Value, Panic> {
        let code = &self.bytecode.codes[code_id as usize];
        let mut frame = Frame {
            registers: vec![Value::Nothing; code.registers as usize],
        };
        for (index, argument) in arguments.into_iter().enumerate() {
            frame.registers[index] = argument;
        }
        self.execute(&mut frame, code_id)
    }

    /// Run one code against a frame until it returns, panics, or falls off the
    /// end.
    fn execute(&self, frame: &mut Frame, code_id: u32) -> Result<Value, Panic> {
        let code = &self.bytecode.codes[code_id as usize];
        let mut pc = 0;
        while pc < code.instructions.len() {
            if self.state.cancelled() {
                return Err(self.fail());
            }
            match &code.instructions[pc] {
                Instruction::LoadConst { dst, constant } => {
                    frame.registers[*dst as usize] =
                        Value::Text(self.bytecode.constants[*constant as usize].clone());
                }
                Instruction::LoadGlobal { dst, global } => {
                    frame.registers[*dst as usize] =
                        self.globals.read_unpoisoned()[*global as usize].clone();
                }
                Instruction::LoadLocal { dst, slot } => {
                    frame.registers[*dst as usize] = frame.registers[*slot as usize].clone();
                }
                Instruction::StoreLocal { slot, src } => {
                    frame.registers[*slot as usize] = frame.registers[*src as usize].clone();
                }
                Instruction::BuildRecord { dst, fields } => {
                    let mut record = Record::default();
                    for (key, value) in fields {
                        let key = self.bytecode.constants[*key as usize].clone();
                        let value = self.text(&frame.registers[*value as usize])?;
                        record.set(key, value);
                    }
                    frame.registers[*dst as usize] = Value::Record(record);
                }
                Instruction::BuildList { dst, elements } => {
                    let mut list = Vec::with_capacity(elements.len());
                    for element in elements {
                        list.push(self.text(&frame.registers[*element as usize])?);
                    }
                    frame.registers[*dst as usize] = Value::List(list);
                }
                Instruction::ListIndex { dst, list, index } => {
                    let element = match (
                        &frame.registers[*list as usize],
                        &frame.registers[*index as usize],
                    ) {
                        (Value::List(list), Value::Number(index)) => {
                            let Some(element) = list.get(*index as usize) else {
                                return Err(self.fail());
                            };
                            element.clone()
                        }
                        _ => return Err(self.fail()),
                    };
                    frame.registers[*dst as usize] = Value::Text(element);
                }
                Instruction::JumpIfIndexAtEnd {
                    target,
                    index,
                    list,
                } => {
                    let at_end = match (
                        &frame.registers[*index as usize],
                        &frame.registers[*list as usize],
                    ) {
                        (Value::Number(index), Value::List(list)) => *index >= list.len() as i64,
                        _ => return Err(self.fail()),
                    };
                    if at_end {
                        pc = *target as usize;
                        continue;
                    }
                }
                Instruction::IncrementNumber { reg } => match &mut frame.registers[*reg as usize] {
                    Value::Number(number) => *number += 1,
                    _ => return Err(self.fail()),
                },
                Instruction::LoadNumber { dst, value } => {
                    frame.registers[*dst as usize] = Value::Number(*value);
                }
                Instruction::SetField { record, key, value } => {
                    let key = self.bytecode.constants[*key as usize].clone();
                    let value = self.text(&frame.registers[*value as usize])?;
                    match &mut frame.registers[*record as usize] {
                        Value::Record(record) => record.set(key, value),
                        _ => return Err(self.fail()),
                    }
                }
                Instruction::GetField { dst, record, key } => {
                    let key = &self.bytecode.constants[*key as usize];
                    let value = match &frame.registers[*record as usize] {
                        Value::Record(record) => Value::Text(record.get(key).to_owned()),
                        _ => return Err(self.fail()),
                    };
                    frame.registers[*dst as usize] = value;
                }
                Instruction::Concat { dst, left, right } => {
                    let left = self.text(&frame.registers[*left as usize])?;
                    let right = self.text(&frame.registers[*right as usize])?;
                    frame.registers[*dst as usize] = Value::Text(left + &right);
                }
                Instruction::CallFunction {
                    dst,
                    code: callee,
                    arguments,
                } => {
                    let values = self.argument_values(frame, arguments);
                    let value = self.run_code(*callee, values)?;
                    frame.registers[*dst as usize] = value;
                }
                Instruction::CallNative {
                    dst,
                    native,
                    arguments,
                } => {
                    let values = self.argument_values(frame, arguments);
                    let value = self.call_native(*native, values)?;
                    frame.registers[*dst as usize] = value;
                }
                Instruction::Return { value } => {
                    return Ok(value
                        .map(|value| frame.registers[value as usize].clone())
                        .unwrap_or(Value::Nothing));
                }
                Instruction::Panic => {
                    self.state.panic();
                    return Err(Panic);
                }
                Instruction::Jump { target } => {
                    pc = *target as usize;
                    continue;
                }
                Instruction::JumpIfEqual {
                    target,
                    left,
                    right,
                } => {
                    let equal = match (
                        &frame.registers[*left as usize],
                        &frame.registers[*right as usize],
                    ) {
                        (Value::Text(left), Value::Text(right)) => left == right,
                        _ => return Err(self.fail()),
                    };
                    if equal {
                        pc = *target as usize;
                        continue;
                    }
                }
                Instruction::Async {
                    code: callee,
                    arguments,
                } => {
                    let values = self.argument_values(frame, arguments);
                    let runtime = self.detached();
                    let callee = *callee;
                    self.state.spawn_thread(move || {
                        if runtime.run_code(callee, values).is_err() {
                            runtime.state.record_failure();
                        }
                    });
                }
                Instruction::AsyncNative { native, arguments } => {
                    let values = self.argument_values(frame, arguments);
                    let runtime = self.detached();
                    let native = *native;
                    self.state.spawn_thread(move || {
                        if runtime.call_native(native, values).is_err() {
                            runtime.state.record_failure();
                        }
                    });
                }
                Instruction::Wait => {
                    self.state.join_children(std::thread::current().id());
                }
            }
            pc += 1;
        }
        Ok(Value::Nothing)
    }

    /// The values of the argument registers, in order.
    fn argument_values(&self, frame: &Frame, arguments: &[u16]) -> Vec<Value> {
        arguments
            .iter()
            .map(|argument| frame.registers[*argument as usize].clone())
            .collect()
    }

    /// The text a value holds, or a failure when it is not text.
    fn text(&self, value: &Value) -> Result<String, Panic> {
        match value {
            Value::Text(text) => Ok(text.clone()),
            Value::Record(_) | Value::List(_) | Value::Nothing | Value::Number(_) => {
                Err(self.fail())
            }
        }
    }

    /// The text of a value, or a failure when it is not text.
    pub(super) fn text_value(&self, value: Value) -> Result<String, Panic> {
        match value {
            Value::Text(text) => Ok(text),
            Value::Record(_) | Value::List(_) | Value::Nothing | Value::Number(_) => {
                Err(self.fail())
            }
        }
    }

    /// A runtime handle that shares the bytecode, globals, and run state, for
    /// one detached thread.
    pub(super) fn detached(&self) -> Runtime {
        Runtime {
            bytecode: Arc::clone(&self.bytecode),
            globals: Arc::clone(&self.globals),
            state: Arc::clone(&self.state),
            #[cfg(test)]
            symbols: HashMap::new(),
        }
    }

    /// The failure that unwinds a body when the checked model is violated.
    /// It only returns the panic; it never signals another body's commands.
    pub(super) fn fail(&self) -> Panic {
        Panic
    }
}
