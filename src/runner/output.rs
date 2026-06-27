use crate::compiler::Sanctuary;
use crate::runner::OutputCallback;
use crate::runner::colors;
use crate::runner::error::RuntimeError;
use crate::runner::parse::ExecContext;
use std::io::{self, Write};
use std::sync::Arc;

type SendWriter = Box<dyn Write + Send>;

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

    pub(crate) fn clone_callback(&self) -> Option<OutputCallback> {
        match self {
            OutputTarget::Callback(cb) => Some(Arc::clone(cb)),
            OutputTarget::Direct(_) => None,
        }
    }
}

pub(crate) struct Runner {
    cfg: Arc<Sanctuary>,
    output: OutputTarget,
}

impl Runner {
    pub(crate) fn new(cfg: Arc<Sanctuary>) -> Self {
        Runner {
            cfg,
            output: OutputTarget::Direct(Box::new(io::stdout()) as SendWriter),
        }
    }

    pub(crate) fn with_output_callback(mut self, callback: OutputCallback) -> Self {
        self.output = OutputTarget::Callback(callback);
        self
    }

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
        ctx.exec_fn_body(fn_body)
    }

    pub(crate) fn execute_standalone_fn(&mut self, fn_name: &str) -> Result<(), RuntimeError> {
        let fn_body = self
            .cfg
            .functions
            .get(fn_name)
            .ok_or_else(|| RuntimeError::Lookup(format!("unknown function: {}", fn_name)))?;

        let mut ctx = ExecContext::new(&self.cfg, None, &mut self.output);
        ctx.exec_fn_body(fn_body)
    }
}
