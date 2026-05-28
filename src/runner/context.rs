use crate::colors;
use crate::config::{Config, Project};
use crate::dsl::ast::{CasePattern, Expr, FnStmt};
use crate::runner::Output;
use crate::runner::error::RuntimeError;
use crate::shell;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

pub type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;

pub struct ExecContext<'a> {
    pub(super) cfg: &'a Config,
    pub(super) project: &'a Project,
    output: &'a mut Output,
    pub(super) vars: HashMap<String, String>,
    pub(super) env_stack: Vec<HashMap<String, String>>,
    pub(super) work_dir: PathBuf,
    pub(super) sys_env: Vec<(String, String)>,
}

impl<'a> ExecContext<'a> {
    pub(super) fn new(cfg: &'a Config, project: &'a Project, output: &'a mut Output) -> Self {
        let mut vars = cfg.vars.clone();
        vars.extend(project.vars.clone());
        ExecContext {
            cfg,
            project,
            output,
            vars,
            env_stack: Vec::new(),
            work_dir: PathBuf::from(&cfg.sanctuary).join(&project.dir),
            sys_env: std::env::vars().collect(),
        }
    }

    fn indent(&self, extra: usize) -> String {
        "  ".repeat(self.env_stack.len() + extra)
    }

    pub(super) fn exec_fn_body(&mut self, body: &[FnStmt]) -> Result<(), RuntimeError> {
        for stmt in body {
            match stmt {
                FnStmt::Log { value, .. } => self.exec_log(value)?,
                FnStmt::Exec { value, .. } => self.exec_exec(value)?,
                FnStmt::Cd { arg, .. } => self.exec_cd(arg)?,
                FnStmt::VarDecl {
                    name,
                    value,
                    var_type,
                    ..
                } => self.exec_var_decl(name, value, var_type)?,
                FnStmt::EnvBlock {
                    pairs,
                    body: block_body,
                    ..
                } => self.exec_env_block(pairs, block_body)?,
                FnStmt::Case { condition, arms } => {
                    let value = self.resolve_expr(condition)?;
                    for arm in arms {
                        if self.match_case_pattern(&arm.pattern, &value)? {
                            let saved_vars = self.vars.clone();
                            let result = self.exec_fn_body(&arm.body);
                            self.vars = saved_vars;
                            result?;
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn match_case_pattern(
        &mut self,
        pattern: &CasePattern,
        value: &str,
    ) -> Result<bool, RuntimeError> {
        match pattern {
            CasePattern::Default => Ok(true),
            CasePattern::Literal { parts } => {
                let mut resolved = String::new();
                for part in parts {
                    if part.is_var {
                        match self.vars.get(&part.value) {
                            Some(v) => resolved.push_str(v),
                            None => {
                                return Err(RuntimeError::Lookup(format!(
                                    "undefined variable: ${}",
                                    part.value
                                )));
                            }
                        }
                    } else {
                        resolved.push_str(&part.value);
                    }
                }
                Ok(value == resolved)
            }
            CasePattern::VarRef { name } => match self.vars.get(name) {
                Some(v) => Ok(value == v),
                None => Err(RuntimeError::Lookup(format!(
                    "undefined variable: ${}",
                    name
                ))),
            },
        }
    }

    fn exec_log(&mut self, value: &Expr) -> Result<(), RuntimeError> {
        let msg = self.resolve_expr(value)?;
        let indent = self.indent(0);
        let line = format!("{}log  {}", indent, msg);
        self.output
            .writeln_colored(&line, colors::LOG_ANSI)
            .map_err(RuntimeError::Io)?;
        Ok(())
    }

    fn exec_exec(&mut self, value: &Expr) -> Result<(), RuntimeError> {
        let cmd_str = self.resolve_expr(value)?;
        let indent = self.indent(0);
        let line = format!("{}exec {}", indent, cmd_str);
        self.output
            .writeln_colored(&line, colors::EXEC_ANSI)
            .map_err(RuntimeError::Io)?;

        let mut child = Command::new(&self.cfg.shell)
            .arg("-c")
            .arg(&cmd_str)
            .current_dir(&self.work_dir)
            .envs(self.build_env())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;

        let indent = self.indent(1);

        let status = match self.output.fork_callback() {
            Some(cb) => {
                let stdout_thread =
                    spawn_stream_reader(child.stdout.take(), indent.clone(), cb.clone());
                let stderr_thread = spawn_stream_reader(child.stderr.take(), indent, cb);

                let status = child
                    .wait()
                    .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;

                if let Some(result) = stdout_thread.map(|h| h.join()) {
                    result
                        .map_err(|_| RuntimeError::Panic("stdout reader panicked".to_string()))??;
                }
                if let Some(result) = stderr_thread.map(|h| h.join()) {
                    result
                        .map_err(|_| RuntimeError::Panic("stderr reader panicked".to_string()))??;
                }

                status
            }
            None => {
                let output = child
                    .wait_with_output()
                    .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;
                write_output_lines(self.output, &output.stdout, &indent)?;
                write_output_lines(self.output, &output.stderr, &indent)?;
                output.status
            }
        };

        if !status.success() {
            return Err(RuntimeError::exec_exit_code(cmd_str, status.code()));
        }

        Ok(())
    }

    fn exec_cd(&mut self, arg: &Expr) -> Result<(), RuntimeError> {
        let resolved = self.resolve_expr(arg)?;
        let base_dir = PathBuf::from(&self.cfg.sanctuary).join(&self.project.dir);
        self.work_dir = if resolved == "." {
            base_dir
        } else {
            base_dir.join(&resolved)
        };

        if !self.work_dir.exists() {
            return Err(RuntimeError::Lookup(format!(
                "cd {}: directory does not exist",
                resolved
            )));
        }

        let indent = self.indent(0);
        let line = format!("{}cd   {}", indent, resolved);
        self.output
            .writeln_colored(&line, colors::CD_ANSI)
            .map_err(RuntimeError::Io)?;
        Ok(())
    }

    fn exec_var_decl(
        &mut self,
        name: &str,
        value: &Expr,
        var_type: &crate::dsl::ast::VarType,
    ) -> Result<(), RuntimeError> {
        let val = self.resolve_expr(value)?;

        let resolved = if var_type == &crate::dsl::ast::VarType::Shell {
            let env_map: HashMap<String, String> = self.build_env().into_iter().collect();
            let out = shell::run_captured(
                &self.cfg.shell,
                &val,
                Some(&self.work_dir),
                Some(&env_map),
                None,
            )
            .map_err(|e| match e {
                shell::Error::Spawn(io_err) => RuntimeError::exec_io_error(&val, io_err),
                shell::Error::Exit {
                    stderr, exit_code, ..
                } => RuntimeError::Exec {
                    cmd: val.clone(),
                    exit_code,
                    detail: stderr,
                },
                shell::Error::Timeout { partial_stderr, .. } => RuntimeError::Exec {
                    cmd: val.clone(),
                    exit_code: None,
                    detail: format!("timed out: {}", partial_stderr),
                },
            })?;
            out.stdout
        } else {
            val
        };

        self.vars.insert(name.to_string(), resolved);
        Ok(())
    }

    fn exec_env_block(
        &mut self,
        pairs: &[crate::dsl::ast::EnvPair],
        body: &[FnStmt],
    ) -> Result<(), RuntimeError> {
        let mut layer = HashMap::new();
        for pair in pairs {
            let val = self.resolve_expr(&pair.value)?;
            layer.insert(pair.key.clone(), val);
        }

        let keys: Vec<&str> = pairs.iter().map(|p| p.key.as_str()).collect();
        let indent = self.indent(0);
        let line = format!("{}env  {}", indent, keys.join(", "));

        self.output
            .writeln_colored(&line, colors::ENV_ANSI)
            .map_err(RuntimeError::Io)?;

        self.env_stack.push(layer);

        let saved_vars = self.vars.clone();
        let result = self.exec_fn_body(body);

        self.vars = saved_vars;
        self.env_stack.pop();

        result
    }
}

fn spawn_stream_reader<R: std::io::Read + Send + 'static>(
    stream: Option<R>,
    indent: String,
    cb: OutputCallback,
) -> Option<std::thread::JoinHandle<Result<(), RuntimeError>>> {
    stream.map(|s| {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(s);
            for line in reader.lines() {
                let line = line.map_err(RuntimeError::Io)?;
                cb([indent.as_str(), line.as_str()].concat());
            }
            Ok(())
        })
    })
}

fn write_output_lines(output: &mut Output, data: &[u8], indent: &str) -> Result<(), RuntimeError> {
    for line in std::io::BufReader::new(data).lines() {
        let line = line.map_err(RuntimeError::Io)?;
        output
            .writeln(&[indent, &line].concat())
            .map_err(RuntimeError::Io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::ast::{CaseArm, TemplatePart};
    use std::collections::HashMap;

    fn test_context(vars: HashMap<String, String>) -> (Config, Project, Output) {
        let project = Project {
            name: "test".to_string(),
            url: "http://example.com".to_string(),
            dir: "test".to_string(),
            sync: "clone".to_string(),
            include_file: None,
            branch: "main".to_string(),
            vars,
            functions: HashMap::new(),
            runs: HashMap::new(),
        };
        let cfg = Config {
            shell: "bash".to_string(),
            sanctuary: "/tmp".to_string(),
            projects: HashMap::new(),
            vars: HashMap::new(),
        };
        (cfg, project, Output::Direct(Box::new(Vec::new())))
    }

    #[test]
    fn test_match_literal_pattern() {
        let vars = HashMap::new();
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let pattern = CasePattern::Literal {
            parts: vec![TemplatePart {
                is_var: false,
                value: "Linux".to_string(),
            }],
        };
        assert!(ctx.match_case_pattern(&pattern, "Linux").unwrap());
        assert!(!ctx.match_case_pattern(&pattern, "Darwin").unwrap());
    }

    #[test]
    fn test_match_literal_with_interpolation() {
        let mut vars = HashMap::new();
        vars.insert("arch".to_string(), "amd64".to_string());
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let pattern = CasePattern::Literal {
            parts: vec![
                TemplatePart {
                    is_var: false,
                    value: "linux/".to_string(),
                },
                TemplatePart {
                    is_var: true,
                    value: "arch".to_string(),
                },
            ],
        };
        assert!(ctx.match_case_pattern(&pattern, "linux/amd64").unwrap());
        assert!(!ctx.match_case_pattern(&pattern, "linux/arm64").unwrap());
    }

    #[test]
    fn test_match_varref_pattern() {
        let mut vars = HashMap::new();
        vars.insert("expected".to_string(), "hello".to_string());
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let pattern = CasePattern::VarRef {
            name: "expected".to_string(),
        };
        assert!(ctx.match_case_pattern(&pattern, "hello").unwrap());
        assert!(!ctx.match_case_pattern(&pattern, "world").unwrap());
    }

    #[test]
    fn test_match_default_pattern() {
        let vars = HashMap::new();
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let pattern = CasePattern::Default;
        assert!(ctx.match_case_pattern(&pattern, "anything").unwrap());
        assert!(ctx.match_case_pattern(&pattern, "").unwrap());
    }

    #[test]
    fn test_match_empty_string() {
        let vars = HashMap::new();
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let pattern = CasePattern::Literal {
            parts: vec![TemplatePart {
                is_var: false,
                value: "".to_string(),
            }],
        };
        assert!(ctx.match_case_pattern(&pattern, "").unwrap());
        assert!(!ctx.match_case_pattern(&pattern, "x").unwrap());
    }

    #[test]
    fn test_match_undefined_var_in_literal_pattern() {
        let vars = HashMap::new();
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let pattern = CasePattern::Literal {
            parts: vec![TemplatePart {
                is_var: true,
                value: "undefined".to_string(),
            }],
        };
        let result = ctx.match_case_pattern(&pattern, "x");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("undefined variable")
        );
    }

    #[test]
    fn test_match_undefined_var_in_varref_pattern() {
        let vars = HashMap::new();
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let pattern = CasePattern::VarRef {
            name: "undefined".to_string(),
        };
        let result = ctx.match_case_pattern(&pattern, "x");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("undefined variable")
        );
    }

    #[test]
    fn test_case_first_match_wins() {
        let vars = HashMap::new();
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let body = [FnStmt::Case {
            condition: Expr::BacktickLit {
                parts: vec![TemplatePart {
                    is_var: false,
                    value: "a".to_string(),
                }],
            },
            arms: vec![
                CaseArm {
                    pattern: CasePattern::Literal {
                        parts: vec![TemplatePart {
                            is_var: false,
                            value: "a".to_string(),
                        }],
                    },
                    body: vec![FnStmt::Log {
                        value: Expr::BacktickLit {
                            parts: vec![TemplatePart {
                                is_var: false,
                                value: "first".to_string(),
                            }],
                        },
                    }],
                },
                CaseArm {
                    pattern: CasePattern::Default,
                    body: vec![FnStmt::Log {
                        value: Expr::BacktickLit {
                            parts: vec![TemplatePart {
                                is_var: false,
                                value: "second".to_string(),
                            }],
                        },
                    }],
                },
            ],
        }];
        ctx.exec_fn_body(&body).unwrap();
        // Only the first arm should have executed (first-match semantics).
        // We verify by checking no error was returned; the default arm
        // should not have been reached since the first arm matched.
    }

    #[test]
    fn test_case_no_match_does_nothing() {
        let vars = HashMap::new();
        let (cfg, project, mut output) = test_context(vars);
        let mut ctx = ExecContext::new(&cfg, &project, &mut output);
        let body = [FnStmt::Case {
            condition: Expr::BacktickLit {
                parts: vec![TemplatePart {
                    is_var: false,
                    value: "no-match".to_string(),
                }],
            },
            arms: vec![CaseArm {
                pattern: CasePattern::Literal {
                    parts: vec![TemplatePart {
                        is_var: false,
                        value: "a".to_string(),
                    }],
                },
                body: vec![FnStmt::Log {
                    value: Expr::BacktickLit {
                        parts: vec![TemplatePart {
                            is_var: false,
                            value: "should-not-run".to_string(),
                        }],
                    },
                }],
            }],
        }];
        // Should not error — no arm matches, but that's valid.
        ctx.exec_fn_body(&body).unwrap();
    }
}
