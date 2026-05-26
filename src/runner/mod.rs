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
        let project = self
            .cfg
            .projects
            .get(project_name)
            .ok_or_else(|| RuntimeError::new(format!("unknown project: {}", project_name)))?;

        let fn_body = project
            .functions
            .get(fn_name)
            .ok_or_else(|| RuntimeError::new(format!("unknown function: {}", fn_name)))?;

        let line = format!("{}({})", fn_name, project_name);
        self.output
            .writeln_colored(&line, colors::EXEC_ANSI)
            .map_err(|e| RuntimeError::new(format!("write error: {}", e)))?;

        let mut ctx = ExecContext::new(&self.cfg, project, &mut self.output);
        ctx.exec_fn_body(fn_body)
    }

    pub fn run_seq(&mut self, seq_name: &str, project_name: &str) -> Result<(), RuntimeError> {
        let fns = self
            .cfg
            .projects
            .get(project_name)
            .ok_or_else(|| RuntimeError::new(format!("unknown project: {}", project_name)))?
            .seqs
            .get(seq_name)
            .ok_or_else(|| RuntimeError::new(format!("unknown seq: {}", seq_name)))?
            .clone();

        let line = format!("seq {} ({})", seq_name, project_name);
        self.output
            .writeln(&line)
            .map_err(|e| RuntimeError::new(format!("write error: {}", e)))?;

        for fn_name in &fns {
            self.execute_fn_call(fn_name, project_name)?;
        }

        Ok(())
    }

    pub fn run_par(&mut self, par_name: &str, project_name: &str) -> Result<(), RuntimeError> {
        let project = self
            .cfg
            .projects
            .get(project_name)
            .ok_or_else(|| RuntimeError::new(format!("unknown project: {}", project_name)))?;

        let fns = project
            .pars
            .get(par_name)
            .ok_or_else(|| RuntimeError::new(format!("unknown par: {}", par_name)))?;

        let line = format!("par {} ({})", par_name, project_name);
        self.output
            .writeln(&line)
            .map_err(|e| RuntimeError::new(format!("write error: {}", e)))?;

        let mut handles = Vec::new();
        let cb = self.output.fork_callback();
        for fn_name in fns {
            let cfg = Arc::clone(&self.cfg);
            let fn_name = fn_name.clone();
            let project_name = project_name.to_string();
            let cb = cb.clone();
            handles.push(std::thread::spawn(move || {
                let mut runner = Runner::from_arc(cfg);
                if let Some(ref cb) = cb {
                    runner = runner.with_output_callback(cb.clone());
                }
                runner.execute_fn_call(&fn_name, &project_name)
            }));
        }

        let mut errors = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => errors.push(e.to_string()),
                Err(_) => errors.push("par task panicked".to_string()),
            }
        }

        if !errors.is_empty() {
            return Err(RuntimeError::new(errors.join("\n")));
        }

        Ok(())
    }
}
