//! Test-only entry points for driving the virtual machine directly.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::compiler::model::Program;
use crate::compiler::{declaration_codes, lower_bytecode};
use crate::runtime::Panic;
use crate::runtime::Value;
use crate::runtime::processes::RuntimeState;

use super::Runtime;

impl Runtime {
    /// Build a runtime over a checked program and evaluate its module values,
    /// exactly as the compiled binary does at startup.
    pub(crate) fn for_testing(program: Program) -> Self {
        let bytecode = lower_bytecode(&program);
        let symbols = root_symbols(&program);
        let global_count = bytecode.module_values.len();
        let runtime = Self {
            bytecode: Arc::new(bytecode),
            globals: Arc::new(RwLock::new(vec![Value::Nothing; global_count])),
            state: Arc::new(RuntimeState::new()),
            symbols,
        };
        runtime
            .evaluate_module_values()
            .expect("module values evaluate at startup");
        runtime
    }

    /// Call a root-namespace function with evaluated arguments.
    pub(crate) fn call_root(&self, name: &str, arguments: Vec<Value>) -> Result<Value, Panic> {
        let code = *self.symbols.get(name).ok_or_else(|| self.fail())?;
        self.run_code(code, arguments)
    }
}

/// The root-namespace function names and their code ids, for the tests that
/// call by name. The code ids come from the one mapping lowering assigns, so
/// the test helper never re-derives the numbering.
fn root_symbols(program: &Program) -> HashMap<String, u32> {
    let root = program.declaration(program.entry).namespace;
    let mut symbols = HashMap::new();
    for (declaration, code_id) in program.declarations.iter().zip(declaration_codes(program)) {
        let Some(code_id) = code_id else {
            continue;
        };
        if declaration.namespace == root && declaration.kind.callable() {
            symbols.insert(declaration.name.clone(), code_id);
        }
    }
    symbols
}
