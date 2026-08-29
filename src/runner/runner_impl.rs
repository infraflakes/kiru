use crate::plan::{Instruction, Plan, Project};
use crate::runner::OutputCallback;
use crate::runner::error::RuntimeError;
use crate::runner::execution_context::ExecContext;
use std::sync::Arc;

/// Executes resolved function bodies against a compiled `Plan`.
pub(crate) struct Runner {
    plan: Arc<Plan>,
    output: OutputCallback,
}

/// Look up a function body by name inside a resolved project.
///
/// Centralizes the function lookup plus its `unknown function` error so the CLI
/// entry point and the runner never diverge on how a missing function is
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

impl Runner {
    /// Create a runner that forwards every emitted output line to `output`.
    pub(crate) fn new(plan: Arc<Plan>, output: OutputCallback) -> Self {
        Runner { plan, output }
    }

    /// Look up and execute a function within a named project.
    pub(crate) fn execute_fn_call(
        &mut self,
        fn_name: &str,
        project_name: &str,
    ) -> Result<(), RuntimeError> {
        let project =
            self.plan.projects.get(project_name).ok_or_else(|| {
                RuntimeError::Lookup(format!("unknown project: {}", project_name))
            })?;

        let fn_body = lookup_project_function_body(project, project_name, fn_name)?;

        let mut ctx = ExecContext::new(&mut self.output, self.plan.shell.clone());
        ctx.exec_stmts(fn_body)
    }
}
