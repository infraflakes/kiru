use crate::exec::OutputCallback;
use crate::exec::context::ExecContext;
use crate::exec::direnv;
use crate::exec::error::RuntimeError;
use crate::exec::subprocess::RunKillSwitch;
use crate::ir::Ir;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// The per-project execution setup derived from a `kiru.toml` project entry:
/// where its commands run and whether they are wrapped in direnv.
pub(crate) struct ProjectExec {
    /// Local working directory of the project. Supports `~` expansion,
    /// already applied by the config loader.
    pub(crate) dir: PathBuf,
    /// When true, every shell command of the project runs through
    /// `direnv exec <dir>`, with the rc approved beforehand.
    pub(crate) direnv: bool,
}

/// Executes resolved function bodies against a compiled `Ir`.
pub(crate) struct Executor {
    ir: Arc<Ir>,
    /// Working directory and direnv setup per project name. Projects
    /// without an entry run at the invocation cwd, plain.
    projects: Arc<BTreeMap<String, ProjectExec>>,
    /// Working directory for projects without a project entry.
    invocation_cwd: PathBuf,
    shell: String,
    timeout: Option<Duration>,
    output: OutputCallback,
    /// Run-level kill switch: a failing chain or a keyboard cancel kills
    /// every live command group of the run.
    kill: Option<Arc<RunKillSwitch>>,
}

impl Executor {
    /// Create an executor that forwards every emitted output line to `output`.
    pub(crate) fn new(
        ir: Arc<Ir>,
        projects: Arc<BTreeMap<String, ProjectExec>>,
        invocation_cwd: PathBuf,
        shell: String,
        timeout: Option<Duration>,
        output: OutputCallback,
        kill: Option<Arc<RunKillSwitch>>,
    ) -> Self {
        Executor {
            ir,
            projects,
            invocation_cwd,
            shell,
            timeout,
            output,
            kill,
        }
    }

    /// Resolve the starting directory and direnv wrap of a project call.
    /// Unlisted projects (or projects without a directory) run at the
    /// invocation cwd, plain.
    fn resolve_project_env(&self, project_name: &str) -> (PathBuf, bool) {
        self.projects
            .get(project_name)
            .map(|project| (project.dir.clone(), project.direnv))
            .unwrap_or_else(|| (self.invocation_cwd.clone(), false))
    }

    /// Look up and execute a function within a named project.
    pub(crate) fn execute_fn_call(
        &mut self,
        fn_name: &str,
        project_name: &str,
    ) -> Result<(), RuntimeError> {
        let (cwd, direnv_wrap) = self.resolve_project_env(project_name);
        // An opted-in project trusts its rc: approve it unconditionally so
        // the wrap below is never rejected as untrusted. direnv's own
        // failures surface as-is, there is no fallback to plain.
        if direnv_wrap {
            direnv::allow_project_env(&cwd, self.timeout, self.kill.as_deref(), None)?;
        }

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

        let mut ctx = ExecContext::new(
            &mut self.output,
            cwd,
            self.shell.clone(),
            self.timeout,
            direnv_wrap,
            self.kill.clone(),
        );
        ctx.exec_stmts(fn_body)
    }
}
