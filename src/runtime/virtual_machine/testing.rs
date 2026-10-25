//! Test-only entry points for driving the virtual machine directly.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::compiler::graph::{DeclarationId, Facts, Program};
use crate::compiler::{declaration_codes, lower_bytecode};
use crate::runtime::Panic;
use crate::runtime::Value;
use crate::runtime::processes::RuntimeState;

use super::Runtime;

impl Runtime {
    /// Build a runtime over the entry's reachable view and evaluate its module
    /// values, exactly as the compiled binary does at startup.
    pub(crate) fn for_testing(
        program: Program,
        facts: Facts,
        entry: DeclarationId,
        reachable: &HashSet<DeclarationId>,
    ) -> Self {
        let bytecode = lower_bytecode(&program, &facts, entry, reachable);
        let symbols = root_symbols(&program, entry, reachable);
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

    /// Build a runtime over every declaration. A test calls root functions
    /// directly, so it must see functions the entry itself never reaches.
    pub(crate) fn for_testing_all(program: Program, facts: Facts, entry: DeclarationId) -> Self {
        let reachable: HashSet<DeclarationId> =
            (0..program.declarations.len()).map(DeclarationId).collect();
        Self::for_testing(program, facts, entry, &reachable)
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
fn root_symbols(
    program: &Program,
    entry: DeclarationId,
    reachable: &HashSet<DeclarationId>,
) -> HashMap<String, u32> {
    let root = program.declaration(entry).namespace;
    let mut symbols = HashMap::new();
    for (declaration, code_id) in program
        .declarations
        .iter()
        .zip(declaration_codes(program, reachable))
    {
        let Some(code_id) = code_id else {
            continue;
        };
        if declaration.namespace == root && declaration.kind.callable() {
            symbols.insert(declaration.name.clone(), code_id);
        }
    }
    symbols
}
