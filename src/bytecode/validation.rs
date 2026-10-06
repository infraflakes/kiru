//! Structural validation of a decoded payload.
//!
//! A program decoded from an executable is untrusted, so every id and index is
//! checked before the VM touches it. Codes are numbered in declaration order,
//! so a call must name a strictly smaller code: a decoded call chain cannot
//! recurse without end.

use super::instruction::{Bytecode, Code, Instruction};

/// Check every id and index in a decoded payload, returning the first
/// violation.
pub(crate) fn validate_bytecode_structure(bytecode: &Bytecode) -> Result<(), String> {
    let code_count = bytecode.codes.len();
    let constant_count = bytecode.constants.len();
    let global_count = bytecode.module_values.len();

    check_id(bytecode.entry as usize, code_count, "the entry")?;
    for code in &bytecode.module_values {
        check_id(*code as usize, code_count, "a module value")?;
    }

    for (index, code) in bytecode.codes.iter().enumerate() {
        for instruction in &code.instructions {
            validate_instruction(
                instruction,
                code,
                index as u32,
                code_count,
                constant_count,
                global_count,
            )?;
        }
    }
    Ok(())
}

fn validate_instruction(
    instruction: &Instruction,
    code: &Code,
    caller: u32,
    code_count: usize,
    constant_count: usize,
    global_count: usize,
) -> Result<(), String> {
    let instruction_count = code.instructions.len();
    let slot = |slot: u16| check_id(slot as usize, code.registers as usize, "a register");
    let constant = |constant: u32| check_id(constant as usize, constant_count, "a constant");
    let global = |global: u32| check_id(global as usize, global_count, "a global");
    // A jump may land one past the last instruction: it falls off the end.
    let target = |target: u32| {
        if target as usize <= instruction_count {
            Ok(())
        } else {
            Err("a jump target points past its table".to_owned())
        }
    };
    let callee = |callee: u32| -> Result<(), String> {
        check_id(callee as usize, code_count, "a call target")?;
        if callee >= caller {
            return Err("a call points at a code that is not earlier".to_owned());
        }
        Ok(())
    };
    match instruction {
        Instruction::LoadConst { dst, constant: k } => {
            slot(*dst)?;
            constant(*k)?;
        }
        Instruction::LoadGlobal { dst, global: g } => {
            slot(*dst)?;
            global(*g)?;
        }
        Instruction::LoadLocal { dst, slot: s } => {
            slot(*dst)?;
            slot(*s)?;
        }
        Instruction::StoreLocal { slot: s, src } => {
            slot(*s)?;
            slot(*src)?;
        }
        Instruction::BuildRecord { dst, fields } => {
            slot(*dst)?;
            for (key, value) in fields {
                constant(*key)?;
                slot(*value)?;
            }
        }
        Instruction::BuildList { dst, elements } => {
            slot(*dst)?;
            for element in elements {
                slot(*element)?;
            }
        }
        Instruction::ListIndex { dst, list, index } => {
            slot(*dst)?;
            slot(*list)?;
            slot(*index)?;
        }
        Instruction::JumpIfIndexAtEnd {
            target: t,
            index,
            list,
        } => {
            target(*t)?;
            slot(*index)?;
            slot(*list)?;
        }
        Instruction::IncrementNumber { reg } => slot(*reg)?,
        Instruction::LoadNumber { dst, .. } => slot(*dst)?,
        Instruction::SetField { record, key, value } => {
            slot(*record)?;
            constant(*key)?;
            slot(*value)?;
        }
        Instruction::GetField { dst, record, key } => {
            slot(*dst)?;
            slot(*record)?;
            constant(*key)?;
        }
        Instruction::Concat { dst, left, right } => {
            slot(*dst)?;
            slot(*left)?;
            slot(*right)?;
        }
        Instruction::CallFunction {
            dst,
            code: callee_code,
            arguments,
        } => {
            slot(*dst)?;
            callee(*callee_code)?;
            for argument in arguments {
                slot(*argument)?;
            }
        }
        Instruction::CallNative {
            dst,
            native: _,
            arguments,
        } => {
            slot(*dst)?;
            for argument in arguments {
                slot(*argument)?;
            }
        }
        Instruction::Return { value } => {
            if let Some(value) = value {
                slot(*value)?;
            }
        }
        Instruction::Panic | Instruction::Wait => {}
        Instruction::Jump { target: t } => target(*t)?,
        Instruction::JumpIfEqual {
            target: t,
            left,
            right,
        } => {
            target(*t)?;
            slot(*left)?;
            slot(*right)?;
        }
        Instruction::Async {
            code: callee_code,
            arguments,
        } => {
            callee(*callee_code)?;
            for argument in arguments {
                slot(*argument)?;
            }
        }
        Instruction::AsyncNative {
            native: _,
            arguments,
        } => {
            for argument in arguments {
                slot(*argument)?;
            }
        }
    }
    Ok(())
}

fn check_id(id: usize, count: usize, what: &str) -> Result<(), String> {
    if id < count {
        Ok(())
    } else {
        Err(format!("{what} points past its table"))
    }
}
