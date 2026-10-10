//! Lowering the validated graph to bytecode.
//!
//! Every function and module value becomes one `Code`. Each
//! parameter and local binding gets a fixed register, and each expression
//! allocates the registers above them. Calls name a code id, references name a
//! register or a global, and a `match` lowers to a compare-and-jump chain
//! that keeps the first-match order.

use std::collections::HashMap;

use crate::bytecode::{Bytecode, Code, Instruction};
use crate::model::{
    DeclarationId, DeclarationKind, Expression, Function, MatchArm, MatchBody, Program, Statement,
    StringPart,
};

/// Lower a validated, pruned program to bytecode.
pub(crate) fn lower_bytecode(program: &Program) -> Bytecode {
    let mut lowerer = Lowerer {
        program,
        codes: Vec::new(),
        constants: Vec::new(),
        constant_index: HashMap::new(),
        module_values: Vec::new(),
        declaration_codes: vec![None; program.declarations.len()],
        declaration_globals: vec![None; program.declarations.len()],
        slots: HashMap::new(),
        instructions: Vec::new(),
        loop_breaks: Vec::new(),
        next: 0,
    };
    lowerer.assign_codes();
    lowerer.lower_all();
    let entry = lowerer.declaration_codes[program.entry.0].expect("the entry has a code");
    Bytecode {
        codes: lowerer.codes,
        constants: lowerer.constants,
        module_values: lowerer.module_values,
        entry,
    }
}

struct Lowerer<'a> {
    program: &'a Program,
    codes: Vec<Code>,
    constants: Vec<String>,
    constant_index: HashMap<String, u32>,
    module_values: Vec<u32>,
    declaration_codes: Vec<Option<u32>>,
    declaration_globals: Vec<Option<u32>>,
    /// The current function's binding registers.
    slots: HashMap<DeclarationId, u16>,
    /// The current function's instructions.
    instructions: Vec<Instruction>,
    /// The `break` jumps of the loops being lowered, innermost last.
    loop_breaks: Vec<Vec<usize>>,
    /// The next free register of the current function.
    next: u16,
}

impl Lowerer<'_> {
    /// Give every declaration with a body a code id, in declaration order. The
    /// entry and every call can then name a code before any body is lowered.
    fn assign_codes(&mut self) {
        for (index, declaration) in self.program.declarations.iter().enumerate() {
            let has_code = matches!(
                declaration.kind,
                DeclarationKind::Function(_) | DeclarationKind::Value { .. }
            );
            if !has_code {
                continue;
            }
            let code_id = self.codes.len() as u32;
            self.declaration_codes[index] = Some(code_id);
            self.codes.push(Code {
                instructions: Vec::new(),
                registers: 0,
                arity: 0,
            });
            if matches!(declaration.kind, DeclarationKind::Value { .. }) {
                self.declaration_globals[index] = Some(self.module_values.len() as u32);
                self.module_values.push(code_id);
            }
        }
    }

    fn lower_all(&mut self) {
        let program = self.program;
        for index in 0..program.declarations.len() {
            let Some(code_id) = self.declaration_codes[index] else {
                continue;
            };
            let declaration = &program.declarations[index];
            let code = match &declaration.kind {
                DeclarationKind::Function(function) => {
                    self.lower_function(function, &declaration.parameters)
                }
                DeclarationKind::Value { expression, .. } => self.lower_initializer(expression),
                DeclarationKind::Binding(_) | DeclarationKind::Native(_) => continue,
            };
            self.codes[code_id as usize] = code;
        }
    }

    fn lower_function(&mut self, function: &Function, parameters: &[DeclarationId]) -> Code {
        self.slots.clear();
        self.instructions.clear();
        self.loop_breaks.clear();
        self.next = parameters.len() as u16;
        for (index, parameter) in parameters.iter().enumerate() {
            self.slots.insert(*parameter, index as u16);
        }
        self.statements(&function.body);
        let registers = self.next;
        Code {
            instructions: std::mem::take(&mut self.instructions),
            registers,
            arity: parameters.len() as u16,
        }
    }

    fn lower_initializer(&mut self, expression: &Expression) -> Code {
        self.slots.clear();
        self.instructions.clear();
        self.next = 0;
        let value = self.expression(expression);
        self.instructions
            .push(Instruction::Return { value: Some(value) });
        Code {
            instructions: std::mem::take(&mut self.instructions),
            registers: self.next,
            arity: 0,
        }
    }

    fn statements(&mut self, statements: &[Statement]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind {
                declaration, value, ..
            }
            | Statement::Assign {
                declaration, value, ..
            } => {
                let src = self.expression(value);
                let slot = self.slot(*declaration);
                self.instructions
                    .push(Instruction::StoreLocal { slot, src });
            }
            Statement::FieldAssign {
                declaration,
                field,
                value,
                ..
            } => {
                let value = self.expression(value);
                let key = self.constant(field);
                let record = self.slot(*declaration);
                self.instructions
                    .push(Instruction::SetField { record, key, value });
            }
            Statement::Expression(expression) => {
                if let Expression::Match {
                    subject,
                    cases,
                    default,
                    ..
                } = expression
                {
                    self.lower_match(subject, cases, default, None);
                } else {
                    let _ = self.expression(expression);
                }
            }
            Statement::Return { value, .. } => {
                let value = value.as_ref().map(|value| self.expression(value));
                self.instructions.push(Instruction::Return { value });
            }
            Statement::Panic { .. } => self.instructions.push(Instruction::Panic),
            Statement::Async { call, .. } => self.lower_async(call),
            Statement::Wait { .. } => self.instructions.push(Instruction::Wait),
            Statement::ForEach {
                item,
                iterable,
                body,
                ..
            } => self.lower_for_each(*item, iterable, body),
            Statement::Forever { body, .. } => self.lower_for(body),
            Statement::Break { .. } => {
                let jump = self.instructions.len();
                self.instructions.push(Instruction::Jump { target: 0 });
                self.loop_breaks
                    .last_mut()
                    .expect("`break` is lowered inside a loop")
                    .push(jump);
            }
        }
    }

    fn expression(&mut self, expression: &Expression) -> u16 {
        match expression {
            Expression::Text { value, .. } => {
                let constant = self.constant(value);
                let dst = self.fresh();
                self.instructions
                    .push(Instruction::LoadConst { dst, constant });
                dst
            }
            Expression::Record { fields, .. } => {
                let mut lowered = Vec::with_capacity(fields.len());
                for field in fields {
                    let value = self.expression(&field.value);
                    let key = self.constant(&field.name);
                    lowered.push((key, value));
                }
                let dst = self.fresh();
                self.instructions.push(Instruction::BuildRecord {
                    dst,
                    fields: lowered,
                });
                dst
            }
            Expression::List { elements, .. } => {
                let lowered: Vec<u16> = elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect();
                let dst = self.fresh();
                self.instructions.push(Instruction::BuildList {
                    dst,
                    elements: lowered,
                });
                dst
            }
            Expression::Interpolated { parts, .. } => self.lower_interpolated(parts),
            Expression::Match {
                subject,
                cases,
                default,
                ..
            } => {
                let dst = self.fresh();
                self.lower_match(subject, cases, default, Some(dst));
                dst
            }
            Expression::Reference { declaration, .. } => {
                let dst = self.fresh();
                let program = self.program;
                match &program.declaration(*declaration).kind {
                    DeclarationKind::Binding(_) => {
                        let slot = self.slot(*declaration);
                        self.instructions.push(Instruction::LoadLocal { dst, slot });
                    }
                    DeclarationKind::Value { .. } => {
                        let global = self.declaration_globals[declaration.0]
                            .expect("a module value has a global");
                        self.instructions
                            .push(Instruction::LoadGlobal { dst, global });
                    }
                    DeclarationKind::Function(_) | DeclarationKind::Native(_) => {
                        unreachable!("a function is not a value")
                    }
                }
                dst
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let dst = self.fresh();
                self.lower_call(*callee, arguments, dst);
                dst
            }
            Expression::Field { target, name, .. } => {
                let record = self.expression(target);
                let key = self.constant(name);
                let dst = self.fresh();
                self.instructions
                    .push(Instruction::GetField { dst, record, key });
                dst
            }
            Expression::Add { left, right, .. } => {
                let left = self.expression(left);
                let right = self.expression(right);
                let dst = self.fresh();
                self.instructions
                    .push(Instruction::Concat { dst, left, right });
                dst
            }
        }
    }

    /// Lower an interpolated string to a chain of concatenations: the first
    /// part seeds the accumulator and each later part is concatenated onto it.
    fn lower_interpolated(&mut self, parts: &[StringPart]) -> u16 {
        let mut accumulator = None;
        for part in parts {
            let register = match part {
                StringPart::Literal(value) => {
                    let constant = self.constant(value);
                    let dst = self.fresh();
                    self.instructions
                        .push(Instruction::LoadConst { dst, constant });
                    dst
                }
                StringPart::Expression(expression) => self.expression(expression),
            };
            accumulator = Some(match accumulator {
                None => register,
                Some(left) => {
                    let dst = self.fresh();
                    self.instructions.push(Instruction::Concat {
                        dst,
                        left,
                        right: register,
                    });
                    dst
                }
            });
        }
        accumulator.unwrap_or_else(|| {
            let constant = self.constant("");
            let dst = self.fresh();
            self.instructions
                .push(Instruction::LoadConst { dst, constant });
            dst
        })
    }

    fn lower_call(&mut self, callee: DeclarationId, arguments: &[Expression], dst: u16) {
        let arguments: Vec<u16> = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect();
        let program = self.program;
        match &program.declaration(callee).kind {
            DeclarationKind::Function(_) => {
                let code = self.declaration_codes[callee.0].expect("a function has a code");
                self.instructions.push(Instruction::CallFunction {
                    dst,
                    code,
                    arguments,
                });
            }
            DeclarationKind::Native(native) => {
                self.instructions.push(Instruction::CallNative {
                    dst,
                    native: *native,
                    arguments,
                });
            }
            _ => unreachable!("only a callable is called"),
        }
    }

    fn lower_async(&mut self, call: &Expression) {
        let Expression::Call {
            callee, arguments, ..
        } = call
        else {
            return;
        };
        let arguments: Vec<u16> = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect();
        let program = self.program;
        match &program.declaration(*callee).kind {
            DeclarationKind::Function(_) => {
                let code = self.declaration_codes[callee.0].expect("a function has a code");
                self.instructions
                    .push(Instruction::Async { code, arguments });
            }
            DeclarationKind::Native(native) => {
                self.instructions.push(Instruction::AsyncNative {
                    native: *native,
                    arguments,
                });
            }
            _ => {}
        }
    }

    /// Lower a `match` to a compare-and-jump chain that keeps the first-match
    /// order. A `dst` register receives the taken arm's value in expression
    /// position; in statement position the taken arm's block runs.
    fn lower_match(
        &mut self,
        subject: &Expression,
        cases: &[MatchArm],
        default: &Option<MatchBody>,
        dst: Option<u16>,
    ) {
        let subject = self.expression(subject);
        let mut ends = Vec::new();
        for arm in cases {
            let pattern = self.expression(&arm.pattern);
            let jump_if_equal = self.instructions.len();
            self.instructions.push(Instruction::JumpIfEqual {
                target: 0,
                left: subject,
                right: pattern,
            });
            let jump_next = self.instructions.len();
            self.instructions.push(Instruction::Jump { target: 0 });
            let body = self.instructions.len() as u32;
            if let Instruction::JumpIfEqual { target, .. } = &mut self.instructions[jump_if_equal] {
                *target = body;
            }
            self.lower_match_body(&arm.body, dst);
            let jump_end = self.instructions.len();
            self.instructions.push(Instruction::Jump { target: 0 });
            ends.push(jump_end);
            let next = self.instructions.len() as u32;
            if let Instruction::Jump { target } = &mut self.instructions[jump_next] {
                *target = next;
            }
        }
        if let Some(default) = default {
            self.lower_match_body(default, dst);
        }
        let end = self.instructions.len() as u32;
        for jump_end in ends {
            if let Instruction::Jump { target } = &mut self.instructions[jump_end] {
                *target = end;
            }
        }
    }

    /// Lower one match arm's body. An expression body writes the arm's value to
    /// `dst` when there is one; a block body runs its statements.
    fn lower_match_body(&mut self, body: &MatchBody, dst: Option<u16>) {
        match body {
            MatchBody::Expression(expression) => {
                let value = self.expression(expression);
                if let Some(dst) = dst {
                    self.instructions.push(Instruction::StoreLocal {
                        slot: dst,
                        src: value,
                    });
                }
            }
            MatchBody::Block(statements) => self.statements(statements),
        }
    }

    /// Lower `for item in <list> { body }` to an index loop. The counter lives
    /// in a register as an internal number; `break` jumps past the loop.
    fn lower_for_each(&mut self, item: DeclarationId, iterable: &Expression, body: &[Statement]) {
        let list = self.expression(iterable);
        let index = self.fresh();
        self.instructions.push(Instruction::LoadNumber {
            dst: index,
            value: 0,
        });

        let head = self.instructions.len() as u32;
        let end_jump = self.instructions.len();
        self.instructions.push(Instruction::JumpIfIndexAtEnd {
            target: 0,
            index,
            list,
        });

        let item_slot = self.slot(item);
        self.instructions.push(Instruction::ListIndex {
            dst: item_slot,
            list,
            index,
        });

        self.loop_breaks.push(Vec::new());
        self.statements(body);
        let breaks = self.loop_breaks.pop().expect("a loop is open");
        self.instructions
            .push(Instruction::IncrementNumber { reg: index });
        self.instructions.push(Instruction::Jump { target: head });

        let end = self.instructions.len() as u32;
        if let Instruction::JumpIfIndexAtEnd { target, .. } = &mut self.instructions[end_jump] {
            *target = end;
        }
        for jump in breaks {
            if let Instruction::Jump { target } = &mut self.instructions[jump] {
                *target = end;
            }
        }
    }

    /// Lower `for { body }` to a head that repeats until a `break`. `break`
    /// jumps past the loop.
    fn lower_for(&mut self, body: &[Statement]) {
        let head = self.instructions.len() as u32;
        self.loop_breaks.push(Vec::new());
        self.statements(body);
        let breaks = self.loop_breaks.pop().expect("a loop is open");
        self.instructions.push(Instruction::Jump { target: head });

        let end = self.instructions.len() as u32;
        for jump in breaks {
            if let Instruction::Jump { target } = &mut self.instructions[jump] {
                *target = end;
            }
        }
    }

    /// The register of a binding, allocating one the first time it is used.
    fn slot(&mut self, declaration: DeclarationId) -> u16 {
        if let Some(&slot) = self.slots.get(&declaration) {
            return slot;
        }
        let slot = self.fresh();
        self.slots.insert(declaration, slot);
        slot
    }

    fn fresh(&mut self) -> u16 {
        let register = self.next;
        self.next += 1;
        register
    }

    fn constant(&mut self, text: &str) -> u32 {
        if let Some(&index) = self.constant_index.get(text) {
            return index;
        }
        let index = self.constants.len() as u32;
        self.constants.push(text.to_owned());
        self.constant_index.insert(text.to_owned(), index);
        index
    }
}
