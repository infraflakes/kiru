//! Lowering the validated graph to bytecode.
//!
//! Every function and module value becomes one `Code`. Each
//! parameter and local binding gets a fixed register, and each expression
//! allocates the registers above them. A function value is made at the point
//! it is used and called through its value; a native is dispatched by index. A
//! reference names a register or a global, and a `match` lowers to a
//! compare-and-jump chain that keeps the first-match order.

use std::collections::HashMap;

use crate::compiler::bytecode::{Bytecode, Code, Instruction, ModuleValue};
use crate::compiler::model::{DeclarationId, Program, Type};
use crate::compiler::native_registry::{NativeSignature, argument_kind};

/// Every lowering fact a declaration carries, read in one pass over the
/// declaration table: its code id, its global when it is a module value, and
/// its signature when it is a native. Only the body walk needs a second pass.
pub(crate) struct DeclarationLayout {
    /// The code id of every declaration, or none when it owns no body.
    codes: Vec<Option<u32>>,
    /// The global that receives each module value, or none.
    globals: Vec<Option<u32>>,
    /// The startup plan, in the resolver's evaluation order.
    module_values: Vec<ModuleValue>,
    /// Every native's parameter shapes, in declaration order.
    natives: Vec<NativeSignature>,
}

impl DeclarationLayout {
    fn of(program: &Program) -> Self {
        let mut codes = vec![None; program.declarations.len()];
        let mut globals = vec![None; program.declarations.len()];
        let mut natives = Vec::new();
        let mut next_code = 0;
        let mut next_global = 0;
        for (index, declaration) in program.declarations.iter().enumerate() {
            // An anonymous function owns no code at layout time: the
            // enclosing body's reference allocates its code where the literal
            // is lowered, so the code order stays the traversal's order.
            let has_code = !declaration.is_anonymous_function()
                && ((declaration.kind.has_body() && declaration.kind.callable())
                    || declaration.kind.module_value());
            if has_code {
                codes[index] = Some(next_code);
                next_code += 1;
            }
            if declaration.kind.module_value() {
                globals[index] = Some(next_global);
                next_global += 1;
            }
            if let Some(native) = declaration.native {
                let Type::Function { parameters, .. } = &declaration.ty else {
                    unreachable!("a native has a function type");
                };
                natives.push(NativeSignature {
                    native,
                    parameters: parameters
                        .iter()
                        .map(|ty| argument_kind(ty, &program.types))
                        .collect(),
                });
            }
        }
        // The startup order is the resolver's dependency order, while the
        // global a value is stored in is its declaration-order slot.
        let module_values = program
            .module_values
            .iter()
            .map(|id| ModuleValue {
                code: codes[id.0].expect("a module value has a code"),
                global: globals[id.0].expect("a module value has a global"),
            })
            .collect();
        Self {
            codes,
            globals,
            module_values,
            natives,
        }
    }
}

/// The code id lowering assigns to each declaration, by declaration id. A
/// function body and a module value each have one; every other declaration has
/// none. The ids follow declaration order, so this one mapping is where that
/// numbering is written and every reader of a code id reads it here.
#[cfg(test)]
pub(crate) fn declaration_codes(program: &Program) -> Vec<Option<u32>> {
    DeclarationLayout::of(program).codes
}

/// Lower a validated, pruned program to bytecode, starting from the entry.
pub(crate) fn lower_bytecode(program: &Program, entry: DeclarationId) -> Bytecode {
    let mut lowerer = Lowerer {
        program,
        codes: Vec::new(),
        constants: Vec::new(),
        constant_index: HashMap::new(),
        module_values: Vec::new(),
        declaration_codes: vec![None; program.declarations.len()],
        declaration_globals: vec![None; program.declarations.len()],
        natives: Vec::new(),
        slots: HashMap::new(),
        instructions: Vec::new(),
        loop_breaks: Vec::new(),
        next: 0,
    };
    lowerer.assign_layout(DeclarationLayout::of(program));
    lowerer.lower_all();
    let entry = lowerer.declaration_codes[entry.0].expect("the entry has a code");
    Bytecode {
        codes: lowerer.codes,
        constants: lowerer.constants,
        module_values: lowerer.module_values,
        entry,
        natives: lowerer.natives,
    }
}

pub(super) struct Lowerer<'a> {
    pub(super) program: &'a Program,
    pub(super) codes: Vec<Code>,
    pub(super) constants: Vec<String>,
    pub(super) constant_index: HashMap<String, u32>,
    pub(super) module_values: Vec<ModuleValue>,
    pub(super) declaration_codes: Vec<Option<u32>>,
    pub(super) declaration_globals: Vec<Option<u32>>,
    pub(super) natives: Vec<NativeSignature>,
    /// The current function's binding registers.
    pub(super) slots: HashMap<DeclarationId, u16>,
    /// The current function's instructions.
    pub(super) instructions: Vec<Instruction>,
    /// The `break` jumps of the loops being lowered, innermost last.
    pub(super) loop_breaks: Vec<Vec<usize>>,
    /// The next free register of the current function.
    pub(super) next: u16,
}

impl Lowerer<'_> {
    /// Install the one declaration scan's result and reserve a code for every
    /// declaration that owns a body. The entry and every call can then name a
    /// code before any body is lowered.
    fn assign_layout(&mut self, layout: DeclarationLayout) {
        self.declaration_codes = layout.codes;
        self.declaration_globals = layout.globals;
        self.module_values = layout.module_values;
        self.natives = layout.natives;
        for code_id in self.declaration_codes.iter().flatten() {
            debug_assert_eq!(
                *code_id as usize,
                self.codes.len(),
                "code ids follow declaration order"
            );
            self.codes.push(Code {
                instructions: Vec::new(),
                registers: 0,
                arity: 0,
            });
        }
    }

    fn lower_all(&mut self) {
        let program = self.program;
        for index in 0..program.declarations.len() {
            let Some(code_id) = self.declaration_codes[index] else {
                continue;
            };
            let declaration = &program.declarations[index];
            let code = if declaration.kind.callable() {
                let function = declaration
                    .function()
                    .expect("a callable body is a function expression");
                self.lower_function(function)
            } else {
                let value = declaration
                    .value()
                    .expect("a body that is not a function is an initializer");
                self.lower_initializer(value)
            };
            self.codes[code_id as usize] = code;
        }
    }
}
