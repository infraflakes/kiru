//! Move a value at its last use.
//!
//! A call consumes the values in its argument registers. When a register is
//! still read after the call, a `Copy` is inserted before it; otherwise the
//! call takes the value and the register is cleared. The runtime's copy-on-write
//! then mutates a unique value in place and copies an aliased one. This is a
//! calling convention over every value, not a rule about one collection.

use std::collections::HashSet;

use crate::compiler::bytecode::{Code, Instruction};

/// Rewrite every code's calls with last-use moves and the copies that keep a
/// still-needed value alive.
pub(super) fn rewrite(codes: &mut [Code]) {
    for code in codes {
        if code.instructions.is_empty() {
            continue;
        }
        let live_out = live_out(&code.instructions);
        let mut next = code.registers;
        let instructions = std::mem::take(&mut code.instructions);
        let old_count = instructions.len();
        let mut rewritten = Vec::with_capacity(old_count);
        // A jump target names the original instruction it lands on. The
        // copies inserted before calls move later instructions, so record
        // where each original instruction begins after the rewrite. The entry
        // for `old_count` is the end of the code.
        let mut moved = vec![0usize; old_count + 1];
        for (index, instruction) in instructions.into_iter().enumerate() {
            moved[index] = rewritten.len();
            let (instruction, copies) = rewrite_call(instruction, &live_out[index], &mut next);
            rewritten.extend(copies);
            rewritten.push(instruction);
        }
        moved[old_count] = rewritten.len();
        for instruction in &mut rewritten {
            remap_jump(instruction, &moved);
        }
        code.instructions = rewritten;
        code.registers = next;
    }
}

/// Rewrite one instruction's jump target from its original instruction index
/// to the index it occupies after copies were inserted.
fn remap_jump(instruction: &mut Instruction, moved: &[usize]) {
    fn target(target: &mut u32, moved: &[usize]) {
        let original = *target as usize;
        if let Some(mapped) = moved.get(original) {
            *target = *mapped as u32;
        }
    }
    match instruction {
        Instruction::Jump { target: jump } => target(jump, moved),
        Instruction::JumpIfFalse { target: jump, .. }
        | Instruction::JumpIfEqual { target: jump, .. } => target(jump, moved),
        Instruction::JumpIfVariantNotEqual { jump, .. } => target(jump, moved),
        _ => {}
    }
}

/// The registers live after each instruction.
fn live_out(instructions: &[Instruction]) -> Vec<HashSet<u16>> {
    let count = instructions.len();
    let mut live_in = vec![HashSet::new(); count];
    loop {
        let mut changed = false;
        for index in (0..count).rev() {
            let mut live = HashSet::new();
            for successor in successors(&instructions[index], index, count) {
                live.extend(live_in[successor].iter().copied());
            }
            for used in uses(&instructions[index]) {
                live.insert(used);
            }
            if let Some(defined) = defined(&instructions[index]) {
                live.remove(&defined);
            }
            if live != live_in[index] {
                live_in[index] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..count)
        .map(|index| {
            let mut live = HashSet::new();
            for successor in successors(&instructions[index], index, count) {
                live.extend(live_in[successor].iter().copied());
            }
            live
        })
        .collect()
}

/// Rewrite one call. A non-call instruction is returned unchanged.
fn rewrite_call(
    instruction: Instruction,
    live: &HashSet<u16>,
    next: &mut u16,
) -> (Instruction, Vec<Instruction>) {
    let mut copies = Vec::new();
    match instruction {
        Instruction::CallNative {
            dst,
            native,
            arguments,
        } => {
            let arguments = move_arguments(arguments, None, Some(dst), live, next, &mut copies);
            (
                Instruction::CallNative {
                    dst,
                    native,
                    arguments,
                },
                copies,
            )
        }
        Instruction::CallValue {
            dst,
            callee,
            arguments,
        } => {
            let arguments =
                move_arguments(arguments, Some(callee), Some(dst), live, next, &mut copies);
            (
                Instruction::CallValue {
                    dst,
                    callee,
                    arguments,
                },
                copies,
            )
        }
        Instruction::AsyncNative { native, arguments } => {
            let arguments = move_arguments(arguments, None, None, live, next, &mut copies);
            (Instruction::AsyncNative { native, arguments }, copies)
        }
        Instruction::AsyncValue { callee, arguments } => {
            let arguments = move_arguments(arguments, Some(callee), None, live, next, &mut copies);
            (Instruction::AsyncValue { callee, arguments }, copies)
        }
        other => (other, copies),
    }
}

/// Keep a register for a call when it is still read later, by a later
/// argument, by the callee, or by the call's destination. Every other
/// argument is a last use and the call takes it.
fn move_arguments(
    arguments: Vec<u16>,
    callee: Option<u16>,
    dst: Option<u16>,
    live: &HashSet<u16>,
    next: &mut u16,
    copies: &mut Vec<Instruction>,
) -> Vec<u16> {
    let mut out = Vec::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().enumerate() {
        let later = arguments[index + 1..].contains(argument);
        let keep =
            later || live.contains(argument) || callee == Some(*argument) || dst == Some(*argument);
        if keep {
            let copy = *next;
            *next = next.checked_add(1).expect("the register count fits u16");
            copies.push(Instruction::Copy {
                dst: copy,
                src: *argument,
            });
            out.push(copy);
        } else {
            out.push(*argument);
        }
    }
    out
}

/// The registers an instruction reads.
fn uses(instruction: &Instruction) -> Vec<u16> {
    match instruction {
        Instruction::LoadConst { .. }
        | Instruction::LoadGlobal { .. }
        | Instruction::LoadNumber { .. }
        | Instruction::LoadBool { .. }
        | Instruction::LoadNothing { .. }
        | Instruction::Jump { .. }
        | Instruction::Panic
        | Instruction::Wait => Vec::new(),
        Instruction::Copy { src, .. } => vec![*src],
        Instruction::StoreLocal { src, .. } => vec![*src],
        Instruction::BuildMap { fields, .. } => fields.iter().map(|(_, value)| *value).collect(),
        Instruction::MakeVariant { payload, .. } => payload.clone(),
        Instruction::JumpIfVariantNotEqual { target, .. } => vec![*target],
        Instruction::GetVariantField { variant, .. } => vec![*variant],
        Instruction::MakeFunction { captures, .. } => captures.clone(),
        Instruction::CallValue {
            callee, arguments, ..
        } => std::iter::once(*callee)
            .chain(arguments.iter().copied())
            .collect(),
        Instruction::NumberOp { left, right, .. } | Instruction::Compare { left, right, .. } => {
            vec![*left, *right]
        }
        Instruction::Negate { src, .. } | Instruction::Not { src, .. } => vec![*src],
        Instruction::JumpIfFalse { condition, .. } => vec![*condition],
        Instruction::ToText { src, .. } => vec![*src],
        Instruction::SetField { map, value, .. } => vec![*map, *value],
        Instruction::GetField { map, .. } => vec![*map],
        Instruction::Concat { left, right, .. } => vec![*left, *right],
        Instruction::CallNative { arguments, .. } => arguments.clone(),
        Instruction::Return { value } => vec![*value],
        Instruction::JumpIfEqual { left, right, .. } => vec![*left, *right],
        Instruction::AsyncNative { arguments, .. } => arguments.clone(),
        Instruction::AsyncValue { callee, arguments } => std::iter::once(*callee)
            .chain(arguments.iter().copied())
            .collect(),
    }
}

/// The register an instruction writes, when it writes one.
fn defined(instruction: &Instruction) -> Option<u16> {
    match instruction {
        Instruction::LoadConst { dst, .. }
        | Instruction::LoadGlobal { dst, .. }
        | Instruction::LoadNumber { dst, .. }
        | Instruction::LoadBool { dst, .. }
        | Instruction::LoadNothing { dst, .. }
        | Instruction::BuildMap { dst, .. }
        | Instruction::MakeVariant { dst, .. }
        | Instruction::GetVariantField { dst, .. }
        | Instruction::MakeFunction { dst, .. }
        | Instruction::CallValue { dst, .. }
        | Instruction::NumberOp { dst, .. }
        | Instruction::Negate { dst, .. }
        | Instruction::Not { dst, .. }
        | Instruction::Compare { dst, .. }
        | Instruction::ToText { dst, .. }
        | Instruction::GetField { dst, .. }
        | Instruction::Concat { dst, .. }
        | Instruction::CallNative { dst, .. }
        | Instruction::Copy { dst, .. } => Some(*dst),
        Instruction::StoreLocal { slot, .. } => Some(*slot),
        Instruction::JumpIfVariantNotEqual { .. }
        | Instruction::JumpIfFalse { .. }
        | Instruction::JumpIfEqual { .. }
        | Instruction::Jump { .. }
        | Instruction::SetField { .. }
        | Instruction::Return { .. }
        | Instruction::Panic
        | Instruction::AsyncNative { .. }
        | Instruction::AsyncValue { .. }
        | Instruction::Wait => None,
    }
}

/// The instruction indices control can reach from one instruction.
fn successors(instruction: &Instruction, index: usize, count: usize) -> Vec<usize> {
    let next = index + 1;
    let target_index = |target: u32| {
        let target = target as usize;
        (target < count).then_some(target)
    };
    match instruction {
        Instruction::Jump { target } => target_index(*target).into_iter().collect(),
        Instruction::JumpIfFalse { target, .. }
        | Instruction::JumpIfEqual { target, .. }
        | Instruction::JumpIfVariantNotEqual { jump: target, .. } => {
            let mut successors = Vec::with_capacity(2);
            if next < count {
                successors.push(next);
            }
            if let Some(target) = target_index(*target) {
                successors.push(target);
            }
            successors
        }
        Instruction::Return { .. } | Instruction::Panic => Vec::new(),
        _ if next < count => vec![next],
        _ => Vec::new(),
    }
}
