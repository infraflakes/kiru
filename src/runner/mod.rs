pub mod context;
pub mod resolver;

use crate::colors;
use crate::config::{Config, ConfigError};
pub use context::{ExecContext, OutputCallback};
use std::io::{self, Write};
use std::sync::Arc;

pub struct Runner {
    cfg: Arc<Config>,
    writer: Box<dyn Write>,
    suppress_headers: bool,
    output_callback: Option<OutputCallback>,
}

impl Runner {
    pub fn new(cfg: Config) -> Self {
        Runner {
            cfg: Arc::new(cfg),
            writer: Box::new(io::stdout()),
            suppress_headers: false,
            output_callback: None,
        }
    }

    pub fn from_arc(cfg: Arc<Config>) -> Self {
        Runner {
            cfg,
            writer: Box::new(io::stdout()),
            suppress_headers: false,
            output_callback: None,
        }
    }

    pub fn with_output_callback(mut self, callback: OutputCallback) -> Self {
        self.output_callback = Some(callback);
        self.writer = Box::new(io::sink());
        self
    }

    pub fn execute_fn_call(
        &mut self,
        fn_name: &str,
        project_name: &str,
    ) -> Result<(), ConfigError> {
        let project =
            self.cfg.projects.get(project_name).ok_or_else(|| {
                ConfigError::Validation(format!("unknown project: {}", project_name))
            })?;

        let fn_body = project
            .functions
            .get(fn_name)
            .ok_or_else(|| ConfigError::Validation(format!("unknown function: {}", fn_name)))?;

        if !self.suppress_headers {
            let line = format!("{}({})", fn_name, project_name);
            if let Some(ref callback) = self.output_callback {
                callback(line);
            } else {
                writeln!(
                    self.writer,
                    "{}{}{}",
                    colors::EXEC_ANSI,
                    line,
                    colors::RESET
                )
                .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
            }
        }

        let mut ctx = ExecContext::new(
            &self.cfg,
            project,
            &mut *self.writer,
            self.output_callback.as_ref(),
        );
        ctx.exec_fn_body(fn_body)
    }

    pub fn run_seq(&mut self, seq_name: &str, project_name: &str) -> Result<(), ConfigError> {
        let fns = self
            .cfg
            .projects
            .get(project_name)
            .ok_or_else(|| ConfigError::Validation(format!("unknown project: {}", project_name)))?
            .seqs
            .get(seq_name)
            .ok_or_else(|| ConfigError::Validation(format!("unknown seq: {}", seq_name)))?
            .clone();

        if !self.suppress_headers {
            let line = format!("seq {} ({})", seq_name, project_name);
            if let Some(ref callback) = self.output_callback {
                callback(line);
            } else {
                writeln!(self.writer, "{}", line)
                    .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
            }
        }

        for fn_name in &fns {
            self.execute_fn_call(fn_name, project_name)?;
        }

        Ok(())
    }

    pub fn run_par(&mut self, par_name: &str, project_name: &str) -> Result<(), ConfigError> {
        let project =
            self.cfg.projects.get(project_name).ok_or_else(|| {
                ConfigError::Validation(format!("unknown project: {}", project_name))
            })?;

        let fns = project
            .pars
            .get(par_name)
            .ok_or_else(|| ConfigError::Validation(format!("unknown par: {}", par_name)))?;

        if !self.suppress_headers {
            let line = format!("par {} ({})", par_name, project_name);
            if let Some(ref callback) = self.output_callback {
                callback(line);
            } else {
                writeln!(self.writer, "{}", line)
                    .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
            }
        }

        let mut handles = Vec::new();
        let callback = self.output_callback.clone();
        for fn_name in fns {
            let cfg = Arc::clone(&self.cfg);
            let fn_name = fn_name.clone();
            let project_name = project_name.to_string();
            let callback = callback.clone();
            handles.push(std::thread::spawn(move || {
                let mut runner = Runner::from_arc(cfg);
                if let Some(ref cb) = callback {
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
            return Err(ConfigError::Validation(errors.join("\n")));
        }

        Ok(())
    }

    pub fn run_fn(&mut self, fn_name: &str, project_name: &str) -> Result<(), ConfigError> {
        self.execute_fn_call(fn_name, project_name)
    }
}
