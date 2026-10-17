//! Lowering the validated graph to bytecode.
//!
//! Every function and module value becomes one `Code`. Each
//! parameter and local binding gets a fixed register, and each expression
//! allocates the registers above them. A function value is made at the point
//! it is used and called through its value; a native is dispatched by index. A
//! reference names a register or a global, and a `match` lowers to a
//! compare-and-jump chain that keeps the first-match order.

use std::collections::HashMap;

use crate::bytecode::{Bytecode, Code, Instruction};
use crate::model::{DeclarationId, Expression, NamespaceId, Program, TypeId};

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
        vec_type: program.vec_type,
        vec_namespace: program.namespace_at(&["std".to_owned(), "dynamic".to_owned()]),
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
pub(super) struct Lowerer<'a> {
    pub(super) program: &'a Program,
    pub(super) codes: Vec<Code>,
    pub(super) constants: Vec<String>,
    pub(super) constant_index: HashMap<String, u32>,
    pub(super) module_values: Vec<u32>,
    pub(super) declaration_codes: Vec<Option<u32>>,
    pub(super) declaration_globals: Vec<Option<u32>>,
    /// The current function's binding registers.
    pub(super) slots: HashMap<DeclarationId, u16>,
    /// The current function's instructions.
    pub(super) instructions: Vec<Instruction>,
    /// The `break` jumps of the loops being lowered, innermost last.
    pub(super) loop_breaks: Vec<Vec<usize>>,
    /// The next free register of the current function.
    pub(super) next: u16,
    /// The runtime's `Vec` type. A `Named` with this id is a list, and this is
    /// the one stage that knows it: the lowering turns the vec operations into
    /// the runtime's list instructions.
    pub(super) vec_type: Option<TypeId>,
    /// The `std::dynamic` namespace, where the vec operations are declared.
    pub(super) vec_namespace: Option<NamespaceId>,
}

impl Lowerer<'_> {
    /// Give every declaration with a value a code id, in declaration order. The
    /// entry and every call can then name a code before any body is lowered. A
    /// module value also gets a global, so it is evaluated once at startup, in
    /// the dependency order the resolver computed.
    fn assign_codes(&mut self) {
        for (index, declaration) in self.program.declarations.iter().enumerate() {
            if declaration.value.is_none() {
                continue;
            }
            let code_id = self.codes.len() as u32;
            self.declaration_codes[index] = Some(code_id);
            self.codes.push(Code {
                instructions: Vec::new(),
                registers: 0,
                arity: 0,
            });
        }
        for id in &self.program.module_values {
            let code_id = self.declaration_codes[id.0].expect("a module value has a code");
            self.declaration_globals[id.0] = Some(self.module_values.len() as u32);
            self.module_values.push(code_id);
        }
    }

    fn lower_all(&mut self) {
        let program = self.program;
        for index in 0..program.declarations.len() {
            let Some(code_id) = self.declaration_codes[index] else {
                continue;
            };
            let declaration = &program.declarations[index];
            let code = match &declaration.value {
                Some(Expression::Function(function)) => self.lower_function(function),
                Some(expression) => self.lower_initializer(expression),
                None => continue,
            };
            self.codes[code_id as usize] = code;
        }
    }
}
