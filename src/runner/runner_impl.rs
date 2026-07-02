use crate::compiler::Config;
use crate::runner::OutputCallback;
use crate::runner::error::RuntimeError;
use crate::runner::execution_context::{ExecContext, OutputTarget};
use std::io;
use std::sync::Arc;

/// Executes resolved function bodies against a compiled `Config`.
pub(crate) struct Runner {
    cfg: Arc<Config>,
    output: OutputTarget,
}

impl Runner {
    /// Create a new runner that writes directly to stdout.
    pub(crate) fn new(cfg: Arc<Config>) -> Self {
        Runner {
            cfg,
            output: OutputTarget::Direct(Box::new(io::stdout())),
        }
    }

    /// Replace output target with a callback (used by the TUI).
    pub(crate) fn with_output_callback(mut self, callback: OutputCallback) -> Self {
        self.output = OutputTarget::Callback(callback);
        self
    }

    /// Look up and execute a function within a named project.
    pub(crate) fn execute_fn_call(
        &mut self,
        fn_name: &str,
        project_name: &str,
    ) -> Result<(), RuntimeError> {
        let project =
            self.cfg.projects.get(project_name).ok_or_else(|| {
                RuntimeError::Lookup(format!("unknown project: {}", project_name))
            })?;

        let fn_body = project
            .functions
            .get(fn_name)
            .ok_or_else(|| RuntimeError::Lookup(format!("unknown function: {}", fn_name)))?;

        let mut ctx = ExecContext::new(Some(project), &mut self.output);
        ctx.exec_stmts(fn_body)
    }
}
