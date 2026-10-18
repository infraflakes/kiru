//! The bytecode virtual machine.
//!
//! A register machine over the lowered program. Module values run once at
//! startup into the globals table, then the entry runs. A function call builds
//! a fresh frame. The `async`
//! keyword starts an OS thread; `wait` joins the threads the calling thread
//! spawned. A native call dispatches in `native_dispatch`, and the test-only
//! entry points live in `testing`.

mod native_dispatch;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

use std::sync::{Arc, RwLock};

#[cfg(test)]
use std::collections::HashMap;

use crate::compiler::bytecode::{Bytecode, CompareOp, Instruction, NumberOp};
use crate::lock_recovery::RwLockExt;
use crate::runtime::Panic;
use crate::runtime::{Map, Text, Value};

use super::processes::{self as process, RuntimeState};

/// Run a program's entry with the words after the program name. Returns the
/// process exit code: zero on success, one on failure or panic, and 130 when
/// a signal arrived.
pub(crate) fn run(bytecode: Arc<Bytecode>, words: &[String]) -> i32 {
    let arguments: Vec<Value> = words
        .iter()
        .cloned()
        .map(|word| Value::Text(word.into()))
        .collect();
    let state = Arc::new(RuntimeState::new());
    // The signal handler keeps a raw pointer to the state, so the state must
    // outlive the run, including a partially installed handler set.
    std::mem::forget(Arc::clone(&state));
    if let Err(error) = process::install_signal_handlers(&state) {
        super::report_error(&format!("cannot install signal handlers: {error}"));
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
    state.threads.join_all();
    if state.interrupted() != 0 {
        130
    } else if state.panicked() {
        1
    } else {
        match entry_status {
            Ok(value) => exit_code(&value),
            Err(_) => 1,
        }
    }
}

/// The process status a program's `main` returned: its `int` clamped to
/// `0..=255`, and `0` when it returned nothing.
fn exit_code(value: &Value) -> i32 {
    match value {
        Value::Int(code) => (*code).clamp(0, 255) as i32,
        _ => 0,
    }
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

    /// Call the entry, passing the command-line words when the entry takes one.
    /// The decode boundary has already rejected an entry with more than one
    /// parameter.
    fn call_entry(&self, words: Vec<Value>) -> Result<Value, Panic> {
        let arity = self.bytecode.codes[self.bytecode.entry as usize].arity;
        let arguments = if arity == 1 {
            vec![Value::Vec(Arc::new(words))]
        } else {
            Vec::new()
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
                        Value::Text(self.bytecode.constants[*constant as usize].clone().into());
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
                Instruction::BuildMap { dst, fields } => {
                    let mut map = Map::default();
                    for (key, value) in fields {
                        let key = self.bytecode.constants[*key as usize].clone();
                        let value = frame.registers[*value as usize].clone();
                        map.set(key, value);
                    }
                    frame.registers[*dst as usize] = Value::Map(map);
                }
                Instruction::MakeVariant { dst, tag, payload } => {
                    let mut values = Vec::with_capacity(payload.len());
                    for element in payload {
                        values.push(frame.registers[*element as usize].clone());
                    }
                    frame.registers[*dst as usize] = Value::Variant {
                        tag: *tag,
                        payload: Arc::new(values),
                    };
                }
                Instruction::LoadNothing { dst } => {
                    frame.registers[*dst as usize] = Value::Nothing;
                }
                Instruction::JumpIfVariantNotEqual { target, tag, jump } => {
                    let not_equal = match &frame.registers[*target as usize] {
                        Value::Variant { tag: actual, .. } => actual != tag,
                        _ => return Err(self.fail()),
                    };
                    if not_equal {
                        pc = *jump as usize;
                        continue;
                    }
                }
                Instruction::JumpIfNothing { value, jump } => {
                    if matches!(frame.registers[*value as usize], Value::Nothing) {
                        pc = *jump as usize;
                        continue;
                    }
                }
                Instruction::JumpIfNotNothing { value, jump } => {
                    if !matches!(frame.registers[*value as usize], Value::Nothing) {
                        pc = *jump as usize;
                        continue;
                    }
                }
                Instruction::GetVariantField {
                    dst,
                    variant,
                    index,
                } => {
                    let value = match &frame.registers[*variant as usize] {
                        Value::Variant { payload, .. } => match payload.get(*index as usize) {
                            Some(value) => value.clone(),
                            None => return Err(self.fail()),
                        },
                        _ => return Err(self.fail()),
                    };
                    frame.registers[*dst as usize] = value;
                }
                Instruction::MakeFunction {
                    dst,
                    code,
                    captures,
                } => {
                    let values: Vec<Value> = captures
                        .iter()
                        .map(|capture| frame.registers[*capture as usize].clone())
                        .collect();
                    frame.registers[*dst as usize] = Value::Function {
                        code: *code,
                        captures: Arc::new(values),
                    };
                }
                Instruction::CallValue {
                    dst,
                    callee,
                    arguments,
                } => {
                    let values = self.argument_values(frame, arguments);
                    let (code, captures) = match &frame.registers[*callee as usize] {
                        Value::Function { code, captures } => (*code, Arc::clone(captures)),
                        _ => return Err(self.fail()),
                    };
                    let mut arguments = captures.as_ref().clone();
                    arguments.extend(values);
                    let value = self.run_code(code, arguments)?;
                    frame.registers[*dst as usize] = value;
                }
                Instruction::LoadNumber { dst, value } => {
                    frame.registers[*dst as usize] = Value::Int(*value);
                }
                Instruction::LoadBool { dst, value } => {
                    frame.registers[*dst as usize] = Value::Bool(*value);
                }
                Instruction::NumberOp {
                    op,
                    dst,
                    left,
                    right,
                } => {
                    let (Value::Int(left), Value::Int(right)) = (
                        &frame.registers[*left as usize],
                        &frame.registers[*right as usize],
                    ) else {
                        return Err(self.fail());
                    };
                    let value = match op {
                        NumberOp::Add => left + right,
                        NumberOp::Subtract => left - right,
                        NumberOp::Multiply => left * right,
                        NumberOp::Divide => {
                            if *right == 0 {
                                return Err(self.fail());
                            }
                            left / right
                        }
                        NumberOp::Remainder => {
                            if *right == 0 {
                                return Err(self.fail());
                            }
                            left % right
                        }
                    };
                    frame.registers[*dst as usize] = Value::Int(value);
                }
                Instruction::Negate { dst, src } => match &frame.registers[*src as usize] {
                    Value::Int(value) => frame.registers[*dst as usize] = Value::Int(-value),
                    _ => return Err(self.fail()),
                },
                Instruction::Not { dst, src } => match &frame.registers[*src as usize] {
                    Value::Bool(value) => frame.registers[*dst as usize] = Value::Bool(!value),
                    _ => return Err(self.fail()),
                },
                Instruction::Compare {
                    op,
                    dst,
                    left,
                    right,
                } => {
                    let left = &frame.registers[*left as usize];
                    let right = &frame.registers[*right as usize];
                    let value = match op {
                        CompareOp::Less
                        | CompareOp::LessEqual
                        | CompareOp::Greater
                        | CompareOp::GreaterEqual => {
                            let ordering = match (left, right) {
                                (Value::Int(left), Value::Int(right)) => left.cmp(right),
                                (Value::Text(left), Value::Text(right)) => {
                                    left.as_str().cmp(right.as_str())
                                }
                                _ => return Err(self.fail()),
                            };
                            match op {
                                CompareOp::Less => ordering.is_lt(),
                                CompareOp::LessEqual => ordering.is_le(),
                                CompareOp::Greater => ordering.is_gt(),
                                _ => ordering.is_ge(),
                            }
                        }
                        CompareOp::Equal | CompareOp::NotEqual => {
                            let equal = left == right;
                            match op {
                                CompareOp::Equal => equal,
                                _ => !equal,
                            }
                        }
                    };
                    frame.registers[*dst as usize] = Value::Bool(value);
                }
                Instruction::JumpIfFalse { target, condition } => {
                    match &frame.registers[*condition as usize] {
                        Value::Bool(false) => {
                            pc = *target as usize;
                            continue;
                        }
                        Value::Bool(true) => {}
                        _ => return Err(self.fail()),
                    }
                }
                Instruction::ToText { dst, src } => {
                    let text = match &frame.registers[*src as usize] {
                        Value::Text(text) => text.clone(),
                        Value::Int(value) => Text::from(value.to_string()),
                        Value::Bool(value) => Text::from(value.to_string()),
                        _ => return Err(self.fail()),
                    };
                    frame.registers[*dst as usize] = Value::Text(text);
                }
                Instruction::SetField { map, key, value } => {
                    let key = self.bytecode.constants[*key as usize].clone();
                    let value = frame.registers[*value as usize].clone();
                    match &mut frame.registers[*map as usize] {
                        Value::Map(map) => map.set(key, value),
                        _ => return Err(self.fail()),
                    }
                }
                Instruction::GetField { dst, map, key } => {
                    let key = &self.bytecode.constants[*key as usize];
                    // A missing key is the absence, never empty text.
                    let value = match &frame.registers[*map as usize] {
                        Value::Map(map) => map.get(key).cloned().unwrap_or(Value::Nothing),
                        _ => return Err(self.fail()),
                    };
                    frame.registers[*dst as usize] = value;
                }
                Instruction::Concat { dst, left, right } => {
                    let left = match &frame.registers[*left as usize] {
                        Value::Text(text) => text.clone(),
                        _ => return Err(self.fail()),
                    };
                    let right = match &frame.registers[*right as usize] {
                        Value::Text(text) => text.clone(),
                        _ => return Err(self.fail()),
                    };
                    frame.registers[*dst as usize] = Value::Text(Text::concat(left, right));
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
                    return Ok(frame.registers[*value as usize].clone());
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
                    // Equality is the one `Value` relation: a match arm compares
                    // the same way `==` does.
                    if frame.registers[*left as usize] == frame.registers[*right as usize] {
                        pc = *target as usize;
                        continue;
                    }
                }
                Instruction::AsyncNative { native, arguments } => {
                    let values = self.argument_values(frame, arguments);
                    let runtime = self.detached();
                    let native = *native;
                    self.state.threads.spawn(move || {
                        if runtime.call_native(native, values).is_err() {
                            runtime.state.record_failure();
                        }
                    });
                }
                Instruction::AsyncValue { callee, arguments } => {
                    let actual = self.argument_values(frame, arguments);
                    let (code, mut values) = match &frame.registers[*callee as usize] {
                        Value::Function { code, captures } => (*code, captures.as_ref().clone()),
                        _ => return Err(self.fail()),
                    };
                    values.extend(actual);
                    let runtime = self.detached();
                    self.state.threads.spawn(move || {
                        if runtime.run_code(code, values).is_err() {
                            runtime.state.record_failure();
                        }
                    });
                }
                Instruction::Wait => {
                    self.state
                        .threads
                        .join_children(std::thread::current().id());
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

    /// The text of a value, or a failure when it is not text.
    pub(super) fn text_value(&self, value: Value) -> Result<String, Panic> {
        match value {
            Value::Text(text) => Ok(text.as_str().to_owned()),
            Value::Map(_)
            | Value::Vec(_)
            | Value::Variant { .. }
            | Value::Function { .. }
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Nothing => Err(self.fail()),
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

    /// The failure for an index outside a Vec. It names the operation, the
    /// index, and the length, so an out-of-bounds `at` or `replace` reports
    /// what went wrong.
    fn index_failure(&self, operation: &str, index: i64, length: usize) -> Panic {
        process::failure(
            &self.state,
            operation,
            &format!("index {index} is outside a Vec of {length} element(s)"),
        )
    }
}
