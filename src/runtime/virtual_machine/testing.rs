//! Test-only entry points for driving the virtual machine directly.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::compiler::lower_bytecode;
use crate::model::{DeclarationKind, Program, Value};
use crate::runtime::Panic;
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
/// call by name. Lowering gives a code to every function, module value, and the
/// entry, in declaration order, so the same walk recovers the ids.
fn root_symbols(program: &Program) -> HashMap<String, u32> {
    let mut code_id = 0;
    let mut symbols = HashMap::new();
    for declaration in &program.declarations {
        let has_code = matches!(
            declaration.kind,
            DeclarationKind::Function(_)
                | DeclarationKind::Text(_)
                | DeclarationKind::Record(_)
                | DeclarationKind::List(_)
        );
        if !has_code {
            continue;
        }
        if declaration.namespace == program.root()
            && declaration.owner.is_none()
            && matches!(declaration.kind, DeclarationKind::Function(_))
        {
            symbols.insert(declaration.name.clone(), code_id);
        }
        code_id += 1;
    }
    symbols
}
