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
    // The runtime passes the entry at most one argument (the command-line
    // words), so an entry with more parameters is not runnable.
    if bytecode.codes[bytecode.entry as usize].arity > 1 {
        return Err("the entry takes more than one parameter".to_owned());
    }
    for code in &bytecode.module_values {
        check_id(*code as usize, code_count, "a module value")?;
    }

    for code in bytecode.codes.iter() {
        for instruction in &code.instructions {
            validate_instruction(instruction, code, code_count, constant_count, global_count)?;
        }
    }
    Ok(())
}

fn validate_instruction(
    instruction: &Instruction,
    code: &Code,
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
    // A closure's code is created after the function that builds it, so
    // `MakeFunction` may point forward; it only needs to be in range.
    let forward = |callee: u32| check_id(callee as usize, code_count, "a function target");
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
        Instruction::BuildMap { dst, fields } => {
            slot(*dst)?;
            for (key, value) in fields {
                constant(*key)?;
                slot(*value)?;
            }
        }
        Instruction::VecNew { dst } => slot(*dst)?,
        Instruction::VecAt { dst, vec, index } => {
            slot(*dst)?;
            slot(*vec)?;
            slot(*index)?;
        }
        Instruction::VecReplace { vec, index, value } => {
            slot(*vec)?;
            slot(*index)?;
            slot(*value)?;
        }
        Instruction::VecPush { vec, item } => {
            slot(*vec)?;
            slot(*item)?;
        }
        Instruction::VecLen { dst, vec } => {
            slot(*dst)?;
            slot(*vec)?;
        }
        Instruction::MakeVariant { dst, payload, .. } => {
            slot(*dst)?;
            for element in payload {
                slot(*element)?;
            }
        }
        Instruction::LoadNothing { dst } => slot(*dst)?,
        Instruction::JumpIfVariantNotEqual {
            target: t, jump, ..
        } => {
            slot(*t)?;
            target(*jump)?;
        }
        Instruction::JumpIfNothing { value, jump } => {
            slot(*value)?;
            target(*jump)?;
        }
        Instruction::JumpIfNotNothing { value, jump } => {
            slot(*value)?;
            target(*jump)?;
        }
        Instruction::GetVariantField { dst, variant, .. } => {
            slot(*dst)?;
            slot(*variant)?;
        }
        Instruction::MakeFunction {
            dst,
            code: target_code,
            captures,
        } => {
            slot(*dst)?;
            forward(*target_code)?;
            for capture in captures {
                slot(*capture)?;
            }
        }
        Instruction::CallValue {
            dst,
            callee,
            arguments,
        } => {
            slot(*dst)?;
            slot(*callee)?;
            for argument in arguments {
                slot(*argument)?;
            }
        }
        Instruction::AsyncValue { callee, arguments } => {
            slot(*callee)?;
            for argument in arguments {
                slot(*argument)?;
            }
        }
        Instruction::LoadNumber { dst, .. } => slot(*dst)?,
        Instruction::LoadBool { dst, .. } => slot(*dst)?,
        Instruction::NumberOp {
            dst, left, right, ..
        } => {
            slot(*dst)?;
            slot(*left)?;
            slot(*right)?;
        }
        Instruction::Negate { dst, src } => {
            slot(*dst)?;
            slot(*src)?;
        }
        Instruction::Not { dst, src } => {
            slot(*dst)?;
            slot(*src)?;
        }
        Instruction::Compare {
            dst, left, right, ..
        } => {
            slot(*dst)?;
            slot(*left)?;
            slot(*right)?;
        }
        Instruction::JumpIfFalse {
            target: t,
            condition,
        } => {
            target(*t)?;
            slot(*condition)?;
        }
        Instruction::ToText { dst, src } => {
            slot(*dst)?;
            slot(*src)?;
        }
        Instruction::SetField { map, key, value } => {
            slot(*map)?;
            constant(*key)?;
            slot(*value)?;
        }
        Instruction::GetField { dst, map, key } => {
            slot(*dst)?;
            slot(*map)?;
            constant(*key)?;
        }
        Instruction::Concat { dst, left, right } => {
            slot(*dst)?;
            slot(*left)?;
            slot(*right)?;
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
        Instruction::Return { value } => slot(*value)?,
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
