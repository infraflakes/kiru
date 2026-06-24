pub(crate) mod error;
pub(crate) mod exec;
pub(crate) mod parse;

#[cfg(test)]
mod tests;

pub(crate) use exec::exec_and_get_stdout;

use crate::colors;
use crate::config::Config;
use error::RuntimeError;
use parse::ExecContext;
pub(crate) use parse::OutputCallback;
use std::io::{self, Write};
use std::sync::Arc;

/// `Output` is `Send` when `Callback` variant is used at runtime.
/// `Direct` variant uses `Box<dyn Write + Send>` to satisfy the bound.
type SendWriter = Box<dyn Write + Send>;

pub(crate) enum Output {
    Direct(SendWriter),
    Callback(OutputCallback),
}

impl Output {
    fn writeln(&mut self, content: &str) -> io::Result<()> {
        match self {
            Output::Direct(w) => writeln!(w, "{content}"),
            Output::Callback(cb) => {
                cb(content.to_string());
                Ok(())
            }
        }
    }

    fn writeln_colored(&mut self, content: &str, color: &str) -> io::Result<()> {
        match self {
            Output::Direct(w) => writeln!(w, "{color}{content}{}", colors::RESET),
            Output::Callback(cb) => {
                cb(content.to_string());
                Ok(())
            }
        }
    }

    pub(crate) fn clone_callback(&self) -> Option<OutputCallback> {
        match self {
            Output::Callback(cb) => Some(Arc::clone(cb)),
            Output::Direct(_) => None,
        }
    }
}

pub(crate) struct Runner {
    cfg: Arc<Config>,
    output: Output,
}

impl Runner {
    pub(crate) fn new(cfg: Config) -> Self {
        Runner {
            cfg: Arc::new(cfg),
            output: Output::Direct(Box::new(io::stdout()) as SendWriter),
        }
    }

    pub(crate) fn with_output_callback(mut self, callback: OutputCallback) -> Self {
        self.output = Output::Callback(callback);
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
