pub(crate) mod context;
pub(crate) mod error;
pub(crate) mod resolver;

use crate::colors;
use crate::config::Config;
use context::ExecContext;
pub use context::OutputCallback;
use error::RuntimeError;
use std::io::{self, Write};
use std::sync::Arc;

pub(crate) enum Output {
    Direct(Box<dyn Write>),
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

    fn fork_callback(&self) -> Option<OutputCallback> {
        match self {
            Output::Callback(cb) => Some(Arc::clone(cb)),
            Output::Direct(_) => None,
        }
    }
}

pub struct Runner {
    cfg: Arc<Config>,
    output: Output,
}

impl Runner {
    pub fn new(cfg: Config) -> Self {
        Runner {
            cfg: Arc::new(cfg),
            output: Output::Direct(Box::new(io::stdout())),
        }
    }

    pub fn from_arc(cfg: Arc<Config>) -> Self {
        Runner {
            cfg,
            output: Output::Direct(Box::new(io::stdout())),
        }
    }

    pub fn with_output_callback(mut self, callback: OutputCallback) -> Self {
        self.output = Output::Callback(callback);
        self
    }

    pub fn execute_fn_call(
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

        let line = format!("{}({})", fn_name, project_name);
        self.output
            .writeln_colored(&line, colors::EXEC_ANSI)
            .map_err(RuntimeError::Io)?;

        let mut ctx = ExecContext::new(&self.cfg, project, &mut self.output);
        ctx.exec_fn_body(fn_body)
    }
}
