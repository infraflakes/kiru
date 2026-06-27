use crate::compiler::Sanctuary;
use crate::runner::OutputCallback;
use crate::runner::colors;
use crate::runner::error::RuntimeError;
use crate::runner::parse::ExecContext;
use std::io::{self, Write};
use std::sync::Arc;

/// A writer that implements `Send` for use across thread boundaries.
type SendWriter = Box<dyn Write + Send>;

/// Where function output is directed: a direct writer or a callback.
pub(crate) enum OutputTarget {
    Direct(SendWriter),
    Callback(OutputCallback),
}

impl OutputTarget {
    pub(super) fn writeln(&mut self, content: &str) -> io::Result<()> {
        match self {
            OutputTarget::Direct(w) => writeln!(w, "{content}"),
            OutputTarget::Callback(cb) => {
                cb(content.to_string());
                Ok(())
            }
        }
    }

    pub(super) fn writeln_colored(&mut self, content: &str, color: &str) -> io::Result<()> {
        match self {
            OutputTarget::Direct(w) => writeln!(w, "{color}{content}{}", colors::RESET),
            OutputTarget::Callback(cb) => {
                cb(content.to_string());
                Ok(())
            }
        }
    }

    /// Clone the callback if this target is a callback variant.
    pub(crate) fn clone_callback(&self) -> Option<OutputCallback> {
        match self {
            OutputTarget::Callback(cb) => Some(Arc::clone(cb)),
            OutputTarget::Direct(_) => None,
        }
    }
}

/// Executes resolved function bodies against a compiled `Sanctuary` config.
pub(crate) struct Runner {
    cfg: Arc<Sanctuary>,
    output: OutputTarget,
}

impl Runner {
    /// Create a new runner that writes directly to stdout.
    pub(crate) fn new(cfg: Arc<Sanctuary>) -> Self {
        Runner {
            cfg,
            output: OutputTarget::Direct(Box::new(io::stdout()) as SendWriter),
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

        let mut ctx = ExecContext::new(&self.cfg, Some(project), &mut self.output);
        ctx.exec_resolved_fn_body(fn_body)
    }

    /// Look up and execute a top-level (standalone) function.
    pub(crate) fn execute_standalone_fn(&mut self, fn_name: &str) -> Result<(), RuntimeError> {
        let fn_body = self
            .cfg
            .functions
            .get(fn_name)
            .ok_or_else(|| RuntimeError::Lookup(format!("unknown function: {}", fn_name)))?;

        let mut ctx = ExecContext::new(&self.cfg, None, &mut self.output);
        ctx.exec_resolved_fn_body(fn_body)
    }
}
