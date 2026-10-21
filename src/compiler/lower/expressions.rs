//! Lowering expressions and statements to instructions.
//!
//! Each expression allocates its result in a fresh register above the frame's
//! parameters and locals. A function literal is lowered into its own code and
//! called through a function value, a `match` becomes a compare-and-jump chain
//! that keeps the first-match order, and a native call is dispatched by index.

use crate::compiler::bytecode::{Code, Instruction};
use crate::compiler::model::{
    DeclarationId, Expression, Function, MatchArm, MatchBody, Pattern, Statement, StringPart,
};
use crate::syntax::{BinaryInstruction, BinaryOp, UnaryOp};

use super::bytecode::Lowerer;

impl Lowerer<'_> {
    /// Lower one function body into a `Code`: its captures then its parameters
    /// take the leading registers.
    pub(super) fn lower_function(&mut self, function: &Function) -> Code {
        self.slots.clear();
        self.instructions.clear();
        self.loop_breaks.clear();
        self.next = 0;
        self.lower_function_body(function)
    }

    /// Lower a function body into the current context, which the caller has
    /// cleared. The captures then the parameters take the leading registers.
    fn lower_function_body(&mut self, function: &Function) -> Code {
        for capture in &function.captures {
            let slot = self.next;
            self.slots.insert(*capture, slot);
            self.next += 1;
        }
        for parameter in &function.parameters {
            let slot = self.next;
            self.slots.insert(*parameter, slot);
            self.next += 1;
        }
        self.statements(&function.body);
        let registers = self.next;
        Code {
            instructions: std::mem::take(&mut self.instructions),
            registers,
            arity: (function.captures.len() + function.parameters.len()) as u16,
        }
    }

    /// Lower a function literal written in expression position: lower its body
    /// into a new code, then make a function value that names that code.
    fn lower_function_expression(&mut self, function: &Function) -> u16 {
        let dst = self.fresh();
        let code = self.codes.len() as u32;
        self.codes.push(Code {
            instructions: Vec::new(),
            registers: 0,
            arity: 0,
        });
        // Lower the nested body in its own register context, then restore the
        // enclosing one before making the function value.
        let saved_slots = std::mem::take(&mut self.slots);
        let saved_instructions = std::mem::take(&mut self.instructions);
        let saved_loop_breaks = std::mem::take(&mut self.loop_breaks);
        let saved_next = self.next;
        self.next = 0;
        let nested = self.lower_function_body(function);
        self.slots = saved_slots;
        self.instructions = saved_instructions;
        self.loop_breaks = saved_loop_breaks;
        self.next = saved_next;
        self.codes[code as usize] = nested;
        self.make_function(dst, code, function);
        dst
    }

    /// Make a function value in `dst`: the code to call and the registers
    /// holding the locals it captured.
    fn make_function(&mut self, dst: u16, code: u32, function: &Function) {
        let captures: Vec<u16> = function
            .captures
            .iter()
            .map(|capture| self.slot(*capture))
            .collect();
        self.instructions.push(Instruction::MakeFunction {
            dst,
            code,
            captures,
        });
    }

    pub(super) fn lower_initializer(&mut self, expression: &Expression) -> Code {
        self.slots.clear();
        self.instructions.clear();
        self.next = 0;
        let value = self.expression(expression);
        self.instructions.push(Instruction::Return { value });
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
                let map = self.slot(*declaration);
                self.instructions
                    .push(Instruction::SetField { map, key, value });
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
                // A bare `return;` returns the no-value marker: it is not a
                // value of any type, and no variant constructs it.
                let value = match value {
                    Some(value) => self.expression(value),
                    None => {
                        let dst = self.fresh();
                        self.instructions.push(Instruction::LoadNothing { dst });
                        dst
                    }
                };
                self.instructions.push(Instruction::Return { value });
            }
            Statement::Panic { .. } => self.instructions.push(Instruction::Panic),
            Statement::Async { call, .. } => self.lower_async(call),
            Statement::Wait { .. } => self.instructions.push(Instruction::Wait),
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
            Expression::Bool { value, .. } => {
                let dst = self.fresh();
                self.instructions
                    .push(Instruction::LoadBool { dst, value: *value });
                dst
            }
            Expression::Int { value, .. } => {
                let dst = self.fresh();
                self.instructions
                    .push(Instruction::LoadNumber { dst, value: *value });
                dst
            }
            Expression::Map { fields, .. } => {
                let mut lowered = Vec::with_capacity(fields.len());
                for field in fields {
                    let value = self.expression(&field.value);
                    let key = self.constant(&field.name);
                    lowered.push((key, value));
                }
                let dst = self.fresh();
                self.instructions.push(Instruction::BuildMap {
                    dst,
                    fields: lowered,
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
            Expression::Reference { declaration, .. } => self.lower_reference(*declaration),
            // The check resolves every family callee it accepts, so lowering
            // reads only the reference edge it wrote.
            Expression::Method { .. } => {
                unreachable!("the check resolved every call's callee");
            }
            Expression::Function(function) => self.lower_function_expression(function),
            Expression::Call {
                callee, arguments, ..
            } => {
                let dst = self.fresh();
                self.lower_call(callee, arguments, dst);
                dst
            }
            Expression::Field { target, name, .. } => {
                let map = self.expression(target);
                let key = self.constant(name);
                let dst = self.fresh();
                self.instructions
                    .push(Instruction::GetField { dst, map, key });
                dst
            }
            Expression::Variant { index, payload, .. } => {
                let mut slots = Vec::with_capacity(payload.len());
                for element in payload {
                    slots.push(self.expression(element));
                }
                let dst = self.fresh();
                self.instructions.push(Instruction::MakeVariant {
                    dst,
                    tag: *index as u16,
                    payload: slots,
                });
                dst
            }
            Expression::Binary {
                op, left, right, ..
            } => self.lower_binary(*op, left, right),
            Expression::Unary { op, operand, .. } => self.lower_unary(*op, operand),
        }
    }

    /// Lower a prefix operator.
    fn lower_unary(&mut self, op: UnaryOp, operand: &Expression) -> u16 {
        let src = self.expression(operand);
        let dst = self.fresh();
        match op {
            UnaryOp::Negate => self.instructions.push(Instruction::Negate { dst, src }),
            UnaryOp::Not => self.instructions.push(Instruction::Not { dst, src }),
        }
        dst
    }

    /// Lower a binary operator. The operator's own facts name the instruction
    /// it becomes; `&&` and `||` short-circuit, and every other operator
    /// evaluates both operands and applies one instruction.
    fn lower_binary(&mut self, op: BinaryOp, left: &Expression, right: &Expression) -> u16 {
        match op.facts().instruction {
            BinaryInstruction::ShortCircuitOr => self.lower_short_circuit(true, left, right),
            BinaryInstruction::ShortCircuitAnd => self.lower_short_circuit(false, left, right),
            BinaryInstruction::Arithmetic(arithmetic) => {
                let left = self.expression(left);
                let right = self.expression(right);
                let dst = self.fresh();
                self.instructions.push(Instruction::NumberOp {
                    op: arithmetic.into(),
                    dst,
                    left,
                    right,
                });
                dst
            }
            BinaryInstruction::Comparison(comparison) => {
                let left = self.expression(left);
                let right = self.expression(right);
                let dst = self.fresh();
                self.instructions.push(Instruction::Compare {
                    op: comparison.into(),
                    dst,
                    left,
                    right,
                });
                dst
            }
        }
    }

    /// Lower `&&` and `||` with short-circuit: the right operand runs only when
    /// the left does not already decide the result. The result lands in one
    /// register, seeded with the left operand.
    fn lower_short_circuit(&mut self, or: bool, left: &Expression, right: &Expression) -> u16 {
        let dst = self.fresh();
        let left = self.expression(left);
        self.instructions.push(Instruction::StoreLocal {
            slot: dst,
            src: left,
        });
        let branch = self.instructions.len();
        self.instructions.push(Instruction::JumpIfFalse {
            target: 0,
            condition: dst,
        });
        if or {
            // A true left decides `||`, so skip the right operand.
            let skip = self.instructions.len();
            self.instructions.push(Instruction::Jump { target: 0 });
            let right_start = self.instructions.len() as u32;
            if let Instruction::JumpIfFalse { target, .. } = &mut self.instructions[branch] {
                *target = right_start;
            }
            let right_value = self.expression(right);
            self.instructions.push(Instruction::StoreLocal {
                slot: dst,
                src: right_value,
            });
            let end = self.instructions.len() as u32;
            if let Instruction::Jump { target } = &mut self.instructions[skip] {
                *target = end;
            }
        } else {
            // A false left decides `&&`, so keep it and skip the right operand.
            let right_value = self.expression(right);
            self.instructions.push(Instruction::StoreLocal {
                slot: dst,
                src: right_value,
            });
            let end = self.instructions.len() as u32;
            if let Instruction::JumpIfFalse { target, .. } = &mut self.instructions[branch] {
                *target = end;
            }
        }
        dst
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
                StringPart::Expression(expression) => {
                    let value = self.expression(expression);
                    let dst = self.fresh();
                    self.instructions
                        .push(Instruction::ToText { dst, src: value });
                    dst
                }
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

    /// Lower a name to a value: a binding reads its register, a module value
    /// reads its global, and a function value makes a function.
    fn lower_reference(&mut self, declaration: DeclarationId) -> u16 {
        let dst = self.fresh();
        let program = self.program;
        let node = program.declaration(declaration);
        if node.kind.callable() && node.kind.has_body() {
            let function = node
                .function()
                .expect("a callable body is a function expression");
            let code = self.declaration_codes[declaration.0].expect("a function has a code");
            self.make_function(dst, code, function);
        } else if node.kind.module_value() {
            let global =
                self.declaration_globals[declaration.0].expect("a module value has a global");
            self.instructions
                .push(Instruction::LoadGlobal { dst, global });
        } else {
            // The checker rejects a native used as a value, because a native has
            // no value form; the invariant is enforced in every build.
            assert!(
                !node.kind.native(),
                "the checker rejects a native used as a value"
            );
            let slot = self.slot(declaration);
            self.instructions.push(Instruction::LoadLocal { dst, slot });
        }
        dst
    }

    /// Lower a call's arguments to registers, in order.
    fn lower_arguments(&mut self, arguments: &[Expression]) -> Vec<u16> {
        arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect()
    }

    /// Lower an application. A native is the one callable with no value form,
    /// so it is dispatched by index; every other callee is an expression whose
    /// value is called.
    fn lower_call(&mut self, callee: &Expression, arguments: &[Expression], dst: u16) {
        if let Expression::Reference { declaration, .. } = callee
            && let Some(native) = self.program.declaration(*declaration).native
        {
            let arguments = self.lower_arguments(arguments);
            self.instructions.push(Instruction::CallNative {
                dst,
                native,
                arguments,
            });
            return;
        }
        let callee = self.expression(callee);
        let arguments = self.lower_arguments(arguments);
        self.instructions.push(Instruction::CallValue {
            dst,
            callee,
            arguments,
        });
    }

    fn lower_async(&mut self, call: &Expression) {
        // The checker owns the shape rule: `async` takes a call. Lowering never
        // sees any other expression, so a non-call is a compiler invariant.
        let Expression::Call {
            callee, arguments, ..
        } = call
        else {
            unreachable!("the checker rejects `async` without a call");
        };
        if let Expression::Reference { declaration, .. } = callee.as_ref()
            && let Some(native) = self.program.declaration(*declaration).native
        {
            let arguments = self.lower_arguments(arguments);
            self.instructions
                .push(Instruction::AsyncNative { native, arguments });
            return;
        }
        let callee = self.expression(callee);
        let arguments = self.lower_arguments(arguments);
        self.instructions
            .push(Instruction::AsyncValue { callee, arguments });
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
        enum Patch {
            Value(usize),
            Variant(usize),
        }
        let mut ends = Vec::new();
        for arm in cases {
            let patch = match &arm.pattern {
                Pattern::Value(expression) => {
                    let pattern = self.expression(expression);
                    let jump_if_equal = self.instructions.len();
                    self.instructions.push(Instruction::JumpIfEqual {
                        target: 0,
                        left: subject,
                        right: pattern,
                    });
                    let jump_next = self.instructions.len();
                    self.instructions.push(Instruction::Jump { target: 0 });
                    // On equality, run the body that follows the `Jump`.
                    if let Instruction::JumpIfEqual { target, .. } =
                        &mut self.instructions[jump_if_equal]
                    {
                        *target = (jump_next + 1) as u32;
                    }
                    Patch::Value(jump_next)
                }
                Pattern::Variant {
                    index, bindings, ..
                } => {
                    let test = self.instructions.len();
                    self.instructions.push(Instruction::JumpIfVariantNotEqual {
                        target: subject,
                        tag: *index as u16,
                        jump: 0,
                    });
                    for (position, binding) in bindings.iter().enumerate() {
                        if let Some(declaration) = binding {
                            let dst = self.slot(*declaration);
                            self.instructions.push(Instruction::GetVariantField {
                                dst,
                                variant: subject,
                                index: position as u16,
                            });
                        }
                    }
                    Patch::Variant(test)
                }
            };
            self.lower_match_body(&arm.body, dst);
            let jump_end = self.instructions.len();
            self.instructions.push(Instruction::Jump { target: 0 });
            ends.push(jump_end);
            let next = self.instructions.len() as u32;
            match patch {
                Patch::Value(at) => {
                    if let Instruction::Jump { target } = &mut self.instructions[at] {
                        *target = next;
                    }
                }
                Patch::Variant(at) => match &mut self.instructions[at] {
                    Instruction::JumpIfVariantNotEqual { jump, .. } => *jump = next,
                    other => unreachable!("a variant arm tests a variant, found {other:?}"),
                },
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
