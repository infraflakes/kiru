use crate::exec::OutputCallback;
use crate::exec::context::ExecContext;
use crate::exec::error::RuntimeError;
use crate::ir::{Instruction, Ir, Project};
use std::sync::Arc;
use std::time::Duration;

/// Executes resolved function bodies against a compiled `Ir`.
pub(crate) struct Executor {
    ir: Arc<Ir>,
    output: OutputCallback,
}

/// Look up a function body by name inside a resolved project.
///
/// Centralizes the function lookup plus its `unknown function` error so the CLI
/// entry point and the executor never diverge on how a missing function is
/// reported.
pub(crate) fn lookup_project_function_body<'a>(
    project: &'a Project,
    project_name: &str,
    fn_name: &str,
) -> Result<&'a [Instruction], RuntimeError> {
    project
        .functions
        .get(fn_name)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            RuntimeError::Lookup(format!(
                "unknown function {} in project {}",
                fn_name, project_name
            ))
        })
}

impl Executor {
    /// Create an executor that forwards every emitted output line to `output`.
    pub(crate) fn new(ir: Arc<Ir>, output: OutputCallback) -> Self {
        Executor { ir, output }
    }

    /// Look up and execute a function within a named project.
    pub(crate) fn execute_fn_call(
        &mut self,
        fn_name: &str,
        project_name: &str,
    ) -> Result<(), RuntimeError> {
        let project =
            self.ir.projects.get(project_name).ok_or_else(|| {
                RuntimeError::Lookup(format!("unknown project: {}", project_name))
            })?;

        let fn_body = lookup_project_function_body(project, project_name, fn_name)?;

        let timeout = Duration::from_secs(self.ir.timeout);
        let mut ctx = ExecContext::new(&mut self.output, self.ir.shell.clone(), timeout);
        ctx.exec_stmts(fn_body)
    }
}
