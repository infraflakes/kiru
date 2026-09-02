use crate::exec::OutputCallback;
use crate::exec::context::ExecContext;
use crate::exec::error::RuntimeError;
use crate::ir::Ir;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Executes resolved function bodies against a compiled `Ir`.
pub(crate) struct Executor {
    ir: Arc<Ir>,
    shell: String,
    timeout: Option<Duration>,
    output: OutputCallback,
}

impl Executor {
    /// Create an executor that forwards every emitted output line to `output`.
    pub(crate) fn new(
        ir: Arc<Ir>,
        shell: String,
        timeout: Option<Duration>,
        output: OutputCallback,
    ) -> Self {
        Executor {
            ir,
            shell,
            timeout,
            output,
        }
    }

    /// Look up and execute a function within a named project.
    pub(crate) fn execute_fn_call(
        &mut self,
        fn_name: &str,
        project_name: &str,
        cwd: PathBuf,
    ) -> Result<(), RuntimeError> {
        let project =
            self.ir.projects.get(project_name).ok_or_else(|| {
                RuntimeError::Lookup(format!("unknown project: {}", project_name))
            })?;

        let fn_body = project
            .functions
            .get(fn_name)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                RuntimeError::Lookup(format!(
                    "unknown function {} in project {}",
                    fn_name, project_name
                ))
            })?;

        let mut ctx = ExecContext::new(&mut self.output, cwd, self.shell.clone(), self.timeout);
        ctx.exec_stmts(fn_body)
    }
}
