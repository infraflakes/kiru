use super::subprocess;
use super::subprocess::RunKillSwitch;
use crate::exec::TaskStatus;
use crate::exec::error::RuntimeError;
use crate::ir::{ArmPattern, EnvPair, Instruction, Segment, Template};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Async bodies started by one body sequence: each handle remembers the
/// display row of the `async()` step that started it, so the step can
/// reflect its child's outcome when the sequence joins.
type SpawnedAsync = Vec<(Option<usize>, JoinHandle<Result<(), RuntimeError>>)>;

/// Emits one output line, tagged with the display row it belongs to
/// (`None` outside any row). This is the only output sink: every execution
/// path supplies one, so there is no separate "write straight to stdout"
/// mode.
pub(crate) type OutputCallback = Arc<dyn Fn(Option<usize>, String) + Send + Sync>;

/// Reports a display row's status. Row indices are assigned at compile time
/// (`Instruction::Step`), so every callback call names its row.
pub(crate) type StatusCallback = Arc<dyn Fn(usize, TaskStatus) + Send + Sync>;

/// Reports a row's resolved label: the runtime values are known, so the
/// display text is the executed data, not its source template.
pub(crate) type LabelCallback = Arc<dyn Fn(usize, String) + Send + Sync>;

/// Reports the resolved project annotation of a row. Project names are
/// templates, so dynamic contexts only know their name at entry.
pub(crate) type ProjectCallback = Arc<dyn Fn(usize, String) + Send + Sync>;

/// Every per-row event sink of one run, grouped so the executor carries one
/// handle and [`ExecContext::fork`] clones one field.
#[derive(Clone)]
pub(crate) struct RowReporter {
    pub(crate) output: OutputCallback,
    pub(crate) status: StatusCallback,
    pub(crate) label: LabelCallback,
    pub(crate) project: ProjectCallback,
}

/// The per-project execution setup derived from a `kiru.toml` project entry:
/// where its commands run and whether they are wrapped in direnv.
#[derive(Debug, Clone)]
pub(crate) struct ProjectExec {
    /// Local working directory of the project. Supports `~` expansion,
    /// already applied by the config loader.
    pub(crate) dir: PathBuf,
    /// When true, every shell command of the project runs through
    /// `direnv exec <dir>`, with the rc approved beforehand.
    pub(crate) direnv: bool,
}

/// What every execution context of one run shares: shell, timeout, the
/// run-level kill switch, the project contexts, and the invocation cwd.
pub(crate) struct RunContext {
    pub(crate) shell: String,
    pub(crate) timeout: Option<Duration>,
    /// Run-level kill switch: a failing task or a keyboard cancel kills
    /// every live command group of the run.
    pub(crate) kill: Option<Arc<RunKillSwitch>>,
    pub(crate) projects: Arc<BTreeMap<String, ProjectExec>>,
}

/// Runtime execution state for a resolved body.
///
/// Variables and arguments are fully inlined at compile time, so there is
/// no runtime scope: every template here is literal text and `$(command)`
/// substitutions. Working directory and environment layers are the
/// context that `cd` and `env` mutate; `async` bodies fork this state so
/// their mutations never leak out.
pub(crate) struct ExecContext {
    run: Arc<RunContext>,
    output: OutputCallback,
    status: StatusCallback,
    label: LabelCallback,
    project: ProjectCallback,
    cwd: PathBuf,
    env_layers: Vec<BTreeMap<String, String>>,
    /// The display row currently being executed, if any. Every line and
    /// nested status is attributed to it.
    row: Option<usize>,
    /// Commands run via `direnv exec <cwd>` when the project opted in with
    /// `direnv = true`. Entering a project context approves the rc first;
    /// direnv itself resolves per-directory rc rules when it runs.
    direnv_wrap: bool,
}

impl ExecContext {
    /// Create the root execution context of a run: `cwd` is the invocation
    /// directory and every display row starts unset.
    pub(crate) fn new(
        run: Arc<RunContext>,
        reporter: RowReporter,
        cwd: PathBuf,
        direnv_wrap: bool,
    ) -> Self {
        ExecContext {
            run,
            output: reporter.output,
            status: reporter.status,
            label: reporter.label,
            project: reporter.project,
            cwd,
            env_layers: Vec::new(),
            row: None,
            direnv_wrap,
        }
    }

    /// A copy of this context for one `async` body: same shared run state
    /// and sink, but its own working directory, environment layers, and
    /// direnv wrapping. Mutations on the copy never leak back.
    fn fork(&self) -> ExecContext {
        ExecContext {
            run: Arc::clone(&self.run),
            output: Arc::clone(&self.output),
            status: Arc::clone(&self.status),
            label: Arc::clone(&self.label),
            project: Arc::clone(&self.project),
            cwd: self.cwd.clone(),
            env_layers: self.env_layers.clone(),
            row: self.row,
            direnv_wrap: self.direnv_wrap,
        }
    }

    /// Whether the run has already been lost to a failure elsewhere.
    fn run_failed(&self) -> bool {
        self.run.kill.as_ref().is_some_and(|kill| kill.is_failed())
    }

    /// The directory argument for `direnv exec` when commands are wrapped,
    /// `None` otherwise.
    fn direnv_dir(&self) -> Option<String> {
        if self.direnv_wrap {
            Some(self.cwd.to_string_lossy().into_owned())
        } else {
            None
        }
    }

    /// Rename the row currently executing with its resolved label.
    fn rename(&self, label: String) {
        if let Some(row) = self.row {
            (self.label)(row, label);
        }
    }

    /// Build the argv for running `cmd` under `shell -c` in the current cwd.
    /// When `direnv_dir` is set, the shell invocation is prefixed with
    /// `direnv exec <dir>` so the project environment applies. `shell` is
    /// passed in (rather than read from `self`) so every argv element shares
    /// one borrow region.
    fn shell_argv<'cmd>(
        &self,
        shell: &'cmd str,
        cmd: &'cmd str,
        direnv_dir: Option<&'cmd str>,
    ) -> Vec<&'cmd str> {
        let mut argv: Vec<&'cmd str> = Vec::new();
        if let Some(dir) = direnv_dir {
            argv.extend_from_slice(&[crate::exec::direnv::DIRENV_PROGRAM, "exec", dir]);
        }
        argv.extend_from_slice(&[shell, "-c", cmd]);
        argv
    }

    /// Resolve a template to a string. When `strict` is true, `$(cmd)` parts
    /// are executed live and must succeed (fatal on non-zero exit or timeout);
    /// otherwise their stdout is captured and inlined, tolerantly returning
    /// empty string on error.
    fn resolve(&self, tmpl: &Template, strict: bool) -> Result<String, RuntimeError> {
        let mut out = String::new();
        for segment in &tmpl.parts {
            match segment {
                Segment::Lit(s) => out.push_str(s),
                Segment::Cmd(inner) => {
                    let cmd = self.resolve(inner, false)?;
                    if strict {
                        self.run_live(&cmd)?;
                    } else {
                        match self.capture(&cmd) {
                            Ok(captured) => {
                                if !captured.is_empty() {
                                    out.push_str(&captured);
                                }
                            }
                            Err(RuntimeError::Timeout { .. }) => {
                                // Tolerant mode: inner timeout silently returns
                                // empty string, no propagation.
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Run a command and stream its output live (fatal on non-zero exit or
    /// timeout). Emits the shell command echo at `log` indent level (0) in
    /// blue, streams output at `log + 1` indent level, and emits timeout
    /// errors via the output callback at the same indent as streaming output.
    fn run_live(&self, cmd: &str) -> Result<(), RuntimeError> {
        let work_dir = &self.cwd;
        let env_overrides: HashMap<String, String> = self.env_overrides();
        let direnv_dir = self.direnv_dir();
        let shell = &self.run.shell;

        let argv = self.shell_argv(shell, cmd, direnv_dir.as_deref());
        let exit = subprocess::run_subprocess(
            cmd,
            &argv,
            Some(work_dir),
            Some(&env_overrides),
            self.run.timeout,
            self.run.kill.as_deref(),
            &mut |line| match line {
                subprocess::SubprocessLine::Stdout(text)
                | subprocess::SubprocessLine::Stderr(text) => {
                    (self.output)(self.row, text.trim_start().to_string());
                }
            },
        );
        match exit {
            Ok(exit) => {
                // The run's stop killed this command: it is a fail-fast
                // victim, not an independent failure.
                if exit.killed_by_switch {
                    return Err(RuntimeError::Cancelled(
                        "stopped because another task failed".to_string(),
                    ));
                }
                if !exit.status.success() {
                    return Err(RuntimeError::exec_io_error(
                        cmd,
                        subprocess::describe_exit_failure(&exit.status),
                    ));
                }
                Ok(())
            }
            Err(subprocess::SubprocessError::Timeout { command, .. }) => {
                let timeout_secs = self.run.timeout.map_or(0, |d| d.as_secs());
                (self.output)(
                    self.row,
                    format!("Error: timeout: command timed out after {timeout_secs}s: {command}"),
                );
                Err(RuntimeError::Timeout {
                    cmd: command,
                    secs: timeout_secs,
                })
            }
            Err(e) => Err(RuntimeError::exec_io_error(cmd, e)),
        }
    }

    /// Run a command and capture its stdout (trimmed). Returns
    /// `Err(Timeout { .. })` when the process exceeds the global timeout; in
    /// that case the captured output so far is discarded. Non-zero exit is
    /// non-fatal.
    fn capture(&self, cmd: &str) -> Result<String, RuntimeError> {
        let env_overrides: HashMap<String, String> = self.env_overrides();
        let direnv_dir = self.direnv_dir();
        let argv = self.shell_argv(self.run.shell.as_str(), cmd, direnv_dir.as_deref());
        subprocess::capture_argv(
            &argv,
            cmd,
            Some(&self.cwd),
            Some(&env_overrides),
            self.run.timeout,
            self.run.kill.as_deref(),
        )
        .map_err(|e| match e {
            subprocess::SubprocessError::Timeout { command, .. } => RuntimeError::Timeout {
                cmd: command,
                secs: self.run.timeout.map_or(0, |d| d.as_secs()),
            },
            other => RuntimeError::exec_io_error(cmd, other),
        })
    }

    /// The environment deltas of every active env-block layer, later layers
    /// winning. The child inherits the rest of the environment from kiru's
    /// own process, so only the overrides are passed to the spawn.
    fn env_overrides(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        for layer in &self.env_layers {
            for (k, v) in layer {
                env.insert(k.clone(), v.clone());
            }
        }
        env
    }

    /// Run a sequence of resolved instructions sequentially, joining the
    /// `async` bodies it started when the sequence ends (structured
    /// concurrency). This is the single execution entry point: `env` and
    /// `switch` bodies, `async` bodies, and the run body itself all recurse
    /// here.
    ///
    /// A `Step` is a display wrapper, not a join scope: asyncs spawned
    /// inside a step (the `async() { ... }` statement itself) belong to
    /// this sequence and are joined here, so sibling statements keep
    /// running while the thread lives.
    ///
    /// Fail-fast: the first genuine failure marks the run failed (killing
    /// every live command group) and marks all pending display rows of this
    /// sequence cancelled.
    pub(crate) fn exec_stmts(&mut self, body: &[Instruction]) -> Result<(), RuntimeError> {
        let mut spawned: SpawnedAsync = Vec::new();
        let failure = self.exec_instructions(body, &mut spawned);

        // Structured concurrency: this body is not finished until every
        // async body it started is finished.
        let mut child_failure: Option<RuntimeError> = None;
        for (owner_row, handle) in spawned {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    // The async header row reflects its child's outcome.
                    if let Some(row) = owner_row {
                        let status = match &error {
                            RuntimeError::Cancelled(_) => TaskStatus::Cancelled,
                            _ => TaskStatus::Error,
                        };
                        (self.status)(row, status);
                    }
                    if child_failure.is_none() {
                        child_failure = Some(error);
                    }
                }
                Err(_) => {
                    if let Some(row) = owner_row {
                        (self.status)(row, TaskStatus::Error);
                    }
                    if child_failure.is_none() {
                        child_failure = Some(RuntimeError::exec_io_error("async", "task panicked"));
                    }
                }
            }
        }

        match (failure, child_failure) {
            (Err(error), _) => Err(error),
            (Ok(()), Some(error)) => Err(error),
            (Ok(()), None) => Ok(()),
        }
    }

    /// The sequential statement loop shared by every body. `spawned` is the
    /// enclosing sequence's async registry, so a step's asyncs outlive the
    /// step and join at the body end.
    fn exec_instructions(
        &mut self,
        body: &[Instruction],
        spawned: &mut SpawnedAsync,
    ) -> Result<(), RuntimeError> {
        for (index, stmt) in body.iter().enumerate() {
            if self.run_failed() {
                cancel_rows_in(&self.status, &body[index..]);
                return Err(RuntimeError::Cancelled(
                    "run failed in another task".to_string(),
                ));
            }
            let result = match stmt {
                Instruction::Step { row, body, .. } => self.exec_step(*row, body, spawned),
                Instruction::Async { body } => self.exec_async(body, spawned),
                Instruction::Context { project, body } => self.exec_context(project, body),
                Instruction::Log(t) => {
                    // A fully literal log already names itself on the step
                    // line; a dynamic one resolves here and renames it, so
                    // the line shows the logged data.
                    if template_is_dynamic(t) {
                        let resolved = self.resolve(t, false)?;
                        self.rename(crate::ir::log_label(&resolved));
                    }
                    Ok(())
                }
                Instruction::Exec { command } => {
                    // One rule: substitutions resolve first, then the
                    // resulting text runs strictly with live output. The
                    // resolved text renames the row, so the display shows
                    // what actually ran.
                    let cmd = self.resolve(command, false)?;
                    if template_is_dynamic(command) {
                        self.rename(crate::ir::exec_label(&cmd));
                    }
                    self.run_live(&cmd)
                }
                Instruction::Cd(t) => {
                    let target = self.resolve(t, false)?;
                    self.exec_cd(&target)?;
                    if template_is_dynamic(t) {
                        self.rename(crate::ir::cd_label(&target));
                    }
                    Ok(())
                }
                Instruction::Env { pairs, body } => self.exec_env_block(pairs, body),
                Instruction::Switch { subject, arms } => {
                    let condition = self.resolve(subject, false)?;
                    let taken = arms.iter().position(|arm| match &arm.pattern {
                        ArmPattern::Lit(pattern) => pattern == &condition,
                        ArmPattern::Default => true,
                    });
                    // Untaken arms end terminal as skipped, so the display
                    // stops waiting for them.
                    for (index, arm) in arms.iter().enumerate() {
                        if Some(index) != taken {
                            skip_rows_in(&self.status, arm.row, &arm.body);
                        }
                    }
                    let Some(index) = taken else {
                        return Ok(());
                    };
                    let arm = &arms[index];
                    (self.status)(arm.row, TaskStatus::Running);
                    let result = self.exec_stmts(&arm.body);
                    let status = match &result {
                        Ok(()) => TaskStatus::Success,
                        Err(RuntimeError::Cancelled(_)) => TaskStatus::Cancelled,
                        Err(_) => TaskStatus::Error,
                    };
                    (self.status)(arm.row, status);
                    result
                }
            };

            if let Err(error) = result {
                if !matches!(error, RuntimeError::Cancelled(_))
                    && let Some(kill) = &self.run.kill
                {
                    kill.fail();
                }
                cancel_rows_in(&self.status, &body[index + 1..]);
                return Err(error);
            }
        }
        Ok(())
    }

    /// Execute one display row: run `body` with every line and status
    /// attributed to `row`. Asyncs started inside the step belong to the
    /// enclosing sequence, not to the step.
    fn exec_step(
        &mut self,
        row: usize,
        body: &[Instruction],
        spawned: &mut SpawnedAsync,
    ) -> Result<(), RuntimeError> {
        (self.status)(row, TaskStatus::Running);
        let previous_row = self.row;
        self.row = Some(row);
        let result = self.exec_instructions(body, spawned);
        self.row = previous_row;

        let status = match &result {
            Ok(()) => TaskStatus::Success,
            Err(RuntimeError::Cancelled(_)) => TaskStatus::Cancelled,
            Err(_) => TaskStatus::Error,
        };
        if let Err(error) = &result
            && !error.is_timeout()
        {
            // Timeout errors were already emitted by `run_live` with the
            // right indent; other failures get one rendered line here.
            (self.output)(Some(row), format!("Error: {error}"));
        }
        (self.status)(row, status);
        result
    }

    /// Start an `async` body now; the enclosing [`Self::exec_stmts`] joins
    /// it when the sequence ends. The body runs on a copy of this context.
    /// The current row (the `async()` step) is recorded so it can reflect
    /// its child's outcome.
    fn exec_async(
        &mut self,
        body: &[Instruction],
        spawned: &mut SpawnedAsync,
    ) -> Result<(), RuntimeError> {
        let mut child = self.fork();
        let body = body.to_vec();
        spawned.push((
            self.row,
            std::thread::spawn(move || child.exec_stmts(&body)),
        ));
        Ok(())
    }

    /// Execute a body in a project's context: resolve the name template,
    /// switch to its directory and direnv setting, then restore both when
    /// the body finishes, so the context never leaks into sibling
    /// statements. A `$()` name resolves here, in the enclosing context.
    fn exec_context(
        &mut self,
        project: &Template,
        body: &[Instruction],
    ) -> Result<(), RuntimeError> {
        // A failure to enter the context happens before any row of the body
        // runs, so it is reported on the body's first row here. Once the
        // body runs, every failure carries the row that produced it and
        // must not be re-reported.
        let (name, entry) = match self.enter_context(project) {
            Ok(entry) => entry,
            Err(error) => {
                report_context_failure(&self.status, &self.output, body, &error);
                return Err(error);
            }
        };
        // A dynamic project name is only known here, so every row of the
        // body is annotated with the resolved name it actually ran under.
        if template_is_dynamic(project) {
            for row in rows_in(body) {
                (self.project)(row, name.clone());
            }
        }
        let previous_cwd = std::mem::replace(&mut self.cwd, entry.dir);
        let previous_wrap = self.direnv_wrap;
        self.direnv_wrap = entry.direnv;
        let result = self.exec_stmts(body);
        self.cwd = previous_cwd;
        self.direnv_wrap = previous_wrap;
        result
    }

    /// Resolve the project name and approve its direnv environment, without
    /// entering the directory yet. Returns the resolved name with its entry.
    fn enter_context(&mut self, project: &Template) -> Result<(String, ProjectExec), RuntimeError> {
        let name = self.resolve(project, false)?;
        if name.is_empty() {
            return Err(RuntimeError::Lookup(
                "project name resolved to empty".to_string(),
            ));
        }
        let entry = self.run.projects.get(&name).cloned().ok_or_else(|| {
            RuntimeError::Lookup(format!(
                "unknown project: `{name}` (no [project.{name}] entry in kiru.toml)"
            ))
        })?;
        if entry.direnv {
            crate::exec::direnv::allow_project_env(
                &entry.dir,
                self.run.timeout,
                self.run.kill.as_deref(),
                None,
            )?;
        }
        Ok((name, entry))
    }

    fn exec_cd(&mut self, target: &str) -> Result<(), RuntimeError> {
        let candidate = if Path::new(target).is_absolute() {
            PathBuf::from(target)
        } else {
            self.cwd.join(target)
        };
        let candidate = std::fs::canonicalize(&candidate)
            .map_err(|e| RuntimeError::Lookup(format!("cd {target}: {e}")))?;
        if !candidate.is_dir() {
            return Err(RuntimeError::Lookup(format!(
                "cd {target}: not a directory"
            )));
        }
        self.cwd = candidate;
        Ok(())
    }

    fn exec_env_block(
        &mut self,
        pairs: &[EnvPair],
        body: &[Instruction],
    ) -> Result<(), RuntimeError> {
        let mut layer = BTreeMap::new();
        for pair in pairs {
            layer.insert(pair.key.clone(), self.resolve(&pair.value, false)?);
        }
        self.env_layers.push(layer);
        let result = self.exec_stmts(body);
        self.env_layers.pop();
        result
    }
}

/// Every display row reachable in `body`, in pre-order: step rows and
/// switch arm rows, descending through nested bodies.
fn rows_in(body: &[Instruction]) -> Vec<usize> {
    fn collect(body: &[Instruction], rows: &mut Vec<usize>) {
        for stmt in body {
            match stmt {
                Instruction::Step { row, body, .. } => {
                    rows.push(*row);
                    collect(body, rows);
                }
                Instruction::Switch { arms, .. } => {
                    for arm in arms {
                        rows.push(arm.row);
                        collect(&arm.body, rows);
                    }
                }
                Instruction::Async { body } | Instruction::Context { body, .. } => {
                    collect(body, rows);
                }
                _ => {}
            }
        }
    }
    let mut rows = Vec::new();
    collect(body, &mut rows);
    rows
}

/// Whether a template resolves at runtime: any `$()` part means its value
/// is not visible on the step line, so it renames the row once resolved.
fn template_is_dynamic(template: &Template) -> bool {
    template
        .parts
        .iter()
        .any(|segment| matches!(segment, Segment::Cmd(_)))
}

/// Mark every display row reachable in `body` with `status`, descending
/// through step bodies, switch arms, async groups, and project contexts.
fn mark_rows(status: &StatusCallback, body: &[Instruction], task_status: TaskStatus) {
    for stmt in body {
        match stmt {
            Instruction::Step { row, body, .. } => {
                (status)(*row, task_status);
                mark_rows(status, body, task_status);
            }
            Instruction::Switch { arms, .. } => {
                for arm in arms {
                    (status)(arm.row, task_status);
                    mark_rows(status, &arm.body, task_status);
                }
            }
            Instruction::Async { body } | Instruction::Context { body, .. } => {
                mark_rows(status, body, task_status);
            }
            _ => {}
        }
    }
}

/// Mark every display row in `body` that has not run yet as cancelled.
fn cancel_rows_in(status: &StatusCallback, body: &[Instruction]) {
    mark_rows(status, body, TaskStatus::Cancelled);
}

/// Mark an untaken switch arm and every step it contains as skipped.
fn skip_rows_in(status: &StatusCallback, arm_row: usize, body: &[Instruction]) {
    (status)(arm_row, TaskStatus::Skipped);
    mark_rows(status, body, TaskStatus::Skipped);
}

/// Report a failure to enter a project context: there is no row for the
/// context itself, so the first row inside it carries the error and the
/// rest are cancelled before the run stops.
fn report_context_failure(
    status: &StatusCallback,
    output: &OutputCallback,
    body: &[Instruction],
    error: &RuntimeError,
) {
    let rows = rows_in(body);
    let Some((first, rest)) = rows.split_first() else {
        return;
    };
    (status)(*first, TaskStatus::Error);
    (output)(Some(*first), format!("Error: {error}"));
    for row in rest {
        (status)(*row, TaskStatus::Cancelled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Arm, Instruction, Segment, Template};

    fn lit(s: &str) -> Template {
        Template {
            parts: vec![Segment::Lit(s.to_string())],
        }
    }

    /// A reporter whose row events are dropped; tests that assert on events
    /// build their own.
    fn silent_reporter() -> RowReporter {
        RowReporter {
            output: Arc::new(|_, _| {}),
            status: Arc::new(|_, _| {}),
            label: Arc::new(|_, _| {}),
            project: Arc::new(|_, _| {}),
        }
    }

    fn test_context(projects: BTreeMap<String, ProjectExec>) -> ExecContext {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let run = Arc::new(RunContext {
            shell: "sh".to_string(),
            timeout: Some(Duration::from_secs(30)),
            kill: None,
            projects: Arc::new(projects),
        });
        ExecContext::new(run, silent_reporter(), cwd, false)
    }

    #[test]
    fn test_switch_first_match() {
        let mut ctx = test_context(BTreeMap::new());
        let body: [Instruction; 1] = [Instruction::Switch {
            subject: lit("a"),
            arms: vec![
                Arm {
                    row: 0,
                    pattern: ArmPattern::Lit("a".to_string()),
                    body: vec![Instruction::Step {
                        row: 1,
                        label: "log first".to_string(),
                        body: vec![Instruction::Log(lit("first"))],
                    }],
                },
                Arm {
                    row: 2,
                    pattern: ArmPattern::Default,
                    body: vec![],
                },
            ],
        }];
        ctx.exec_stmts(&body).unwrap();
    }

    #[test]
    fn test_async_runs_and_joins() {
        let mut ctx = test_context(BTreeMap::new());
        let body: [Instruction; 1] = [Instruction::Async {
            body: vec![Instruction::Step {
                row: 0,
                label: "work".to_string(),
                body: vec![Instruction::Log(lit("async ran"))],
            }],
        }];
        ctx.exec_stmts(&body).unwrap();
    }

    #[test]
    fn test_context_unknown_project_errors() {
        let mut ctx = test_context(BTreeMap::new());
        let body: [Instruction; 1] = [Instruction::Context {
            project: lit("missing"),
            body: vec![],
        }];
        let error = ctx.exec_stmts(&body).unwrap_err();
        assert!(error.to_string().contains("unknown project"), "{error}");
    }

    #[test]
    fn test_context_switch_restores_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let project_dir = dir.path().join("proj");
        std::fs::create_dir_all(&project_dir).unwrap();
        let mut projects = BTreeMap::new();
        projects.insert(
            "p".to_string(),
            ProjectExec {
                dir: project_dir.clone(),
                direnv: false,
            },
        );
        let mut ctx = test_context(projects);
        let body: [Instruction; 1] = [Instruction::Context {
            project: lit("p"),
            body: vec![Instruction::Cd(lit("."))],
        }];
        ctx.exec_stmts(&body).unwrap();
        // After the context finishes, the working directory is the
        // invocation one again.
        let invocation = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        assert_eq!(ctx.cwd, invocation, "context cwd must not leak out");
    }

    #[test]
    fn test_context_failure_is_reported_on_its_first_row() {
        let statuses: Arc<std::sync::Mutex<Vec<(usize, TaskStatus)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let outputs: Arc<std::sync::Mutex<Vec<(Option<usize>, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let status_recorder = Arc::clone(&statuses);
        let output_recorder = Arc::clone(&outputs);
        let status: StatusCallback =
            Arc::new(move |row, status| status_recorder.lock().unwrap().push((row, status)));
        let output: OutputCallback =
            Arc::new(move |row, line| output_recorder.lock().unwrap().push((row, line)));

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let run = Arc::new(RunContext {
            shell: "sh".to_string(),
            timeout: Some(Duration::from_secs(30)),
            kill: None,
            projects: Arc::new(BTreeMap::new()),
        });
        let reporter = RowReporter {
            output,
            status,
            label: Arc::new(|_, _| {}),
            project: Arc::new(|_, _| {}),
        };
        let mut ctx = ExecContext::new(run, reporter, cwd, false);

        let body: [Instruction; 1] = [Instruction::Context {
            project: lit("missing"),
            body: vec![
                Instruction::Step {
                    row: 0,
                    label: "a".to_string(),
                    body: vec![],
                },
                Instruction::Step {
                    row: 1,
                    label: "b".to_string(),
                    body: vec![],
                },
            ],
        }];
        let error = ctx.exec_stmts(&body).unwrap_err();
        assert!(error.to_string().contains("unknown project"), "{error}");
        let statuses = statuses.lock().unwrap();
        assert!(
            statuses.contains(&(0, TaskStatus::Error)),
            "first row carries the error: {statuses:?}"
        );
        assert!(
            statuses.contains(&(1, TaskStatus::Cancelled)),
            "later rows are cancelled: {statuses:?}"
        );
        let outputs = outputs.lock().unwrap();
        assert!(
            outputs
                .iter()
                .any(|(row, line)| *row == Some(0) && line.contains("unknown project")),
            "the error is rendered on the first row: {outputs:?}"
        );
    }

    #[test]
    fn test_untaken_switch_arms_are_skipped() {
        let statuses: Arc<std::sync::Mutex<Vec<(usize, TaskStatus)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let status_recorder = Arc::clone(&statuses);
        let status: StatusCallback =
            Arc::new(move |row, status| status_recorder.lock().unwrap().push((row, status)));
        let output: OutputCallback = Arc::new(|_, _| {});

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let run = Arc::new(RunContext {
            shell: "sh".to_string(),
            timeout: Some(Duration::from_secs(30)),
            kill: None,
            projects: Arc::new(BTreeMap::new()),
        });
        let reporter = RowReporter {
            output,
            status,
            label: Arc::new(|_, _| {}),
            project: Arc::new(|_, _| {}),
        };
        let mut ctx = ExecContext::new(run, reporter, cwd, false);

        let log = |row: usize, text: &str| Instruction::Step {
            row,
            label: format!("log: {text}"),
            body: vec![Instruction::Log(lit(text))],
        };
        let body: [Instruction; 1] = [Instruction::Switch {
            subject: lit("a"),
            arms: vec![
                Arm {
                    row: 0,
                    pattern: ArmPattern::Lit("a".to_string()),
                    body: vec![log(1, "taken")],
                },
                Arm {
                    row: 2,
                    pattern: ArmPattern::Default,
                    body: vec![log(3, "untaken")],
                },
            ],
        }];
        ctx.exec_stmts(&body).unwrap();

        let statuses = statuses.lock().unwrap();
        assert!(
            statuses.contains(&(0, TaskStatus::Success)),
            "taken arm succeeds: {statuses:?}"
        );
        assert!(
            statuses.contains(&(2, TaskStatus::Skipped)),
            "untaken arm is skipped: {statuses:?}"
        );
        assert!(
            statuses.contains(&(3, TaskStatus::Skipped)),
            "untaken arm steps are skipped: {statuses:?}"
        );
        assert!(
            !statuses.contains(&(3, TaskStatus::Running)),
            "skipped steps never run: {statuses:?}"
        );
    }

    /// Parenthesized content is data: a dynamic template resolves when it
    /// executes and replaces the row's plan label with the data.
    #[test]
    fn test_dynamic_labels_resolve_at_runtime() {
        let labels: Arc<std::sync::Mutex<Vec<(usize, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let outputs: Arc<std::sync::Mutex<Vec<(Option<usize>, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let label_recorder = Arc::clone(&labels);
        let output_recorder = Arc::clone(&outputs);
        let reporter = RowReporter {
            output: Arc::new(move |row, line| {
                output_recorder.lock().unwrap().push((row, line));
            }),
            status: Arc::new(|_, _| {}),
            label: Arc::new(move |row, name| {
                label_recorder.lock().unwrap().push((row, name));
            }),
            project: Arc::new(|_, _| {}),
        };

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let run = Arc::new(RunContext {
            shell: "sh".to_string(),
            timeout: Some(Duration::from_secs(30)),
            kill: None,
            projects: Arc::new(BTreeMap::new()),
        });
        let mut ctx = ExecContext::new(run, reporter, cwd, false);

        let dynamic = || Template {
            parts: vec![
                Segment::Lit("echo ".to_string()),
                Segment::Cmd(lit("printf hi")),
            ],
        };
        let body: [Instruction; 2] = [
            Instruction::Step {
                row: 0,
                label: "exec: echo $(printf hi)".to_string(),
                body: vec![Instruction::Exec { command: dynamic() }],
            },
            Instruction::Step {
                row: 1,
                label: "log: $(printf hi)".to_string(),
                body: vec![Instruction::Log(Template {
                    parts: vec![Segment::Cmd(lit("printf hi"))],
                })],
            },
        ];
        ctx.exec_stmts(&body).unwrap();

        let labels = labels.lock().unwrap();
        assert!(
            labels.contains(&(0, "exec: echo hi".to_string())),
            "the executed command renames its row: {labels:?}"
        );
        assert!(
            labels.contains(&(1, "log: hi".to_string())),
            "the logged data renames its row: {labels:?}"
        );
        let outputs = outputs.lock().unwrap();
        assert!(
            !outputs.iter().any(|(row, _)| *row == Some(1)),
            "a resolved log is its label, not an output line: {outputs:?}"
        );
    }

    /// A dynamic project name is only known at entry; every row of the body
    /// is annotated with the name it actually ran under.
    #[test]
    fn test_dynamic_project_name_annotates_body_rows() {
        let updates: Arc<std::sync::Mutex<Vec<(usize, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let update_recorder = Arc::clone(&updates);
        let reporter = RowReporter {
            output: Arc::new(|_, _| {}),
            status: Arc::new(|_, _| {}),
            label: Arc::new(|_, _| {}),
            project: Arc::new(move |row, name| {
                update_recorder.lock().unwrap().push((row, name));
            }),
        };

        let dir = tempfile::tempdir().unwrap();
        let mut projects = BTreeMap::new();
        projects.insert(
            "app".to_string(),
            ProjectExec {
                dir: dir.path().to_path_buf(),
                direnv: false,
            },
        );
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let run = Arc::new(RunContext {
            shell: "sh".to_string(),
            timeout: Some(Duration::from_secs(30)),
            kill: None,
            projects: Arc::new(projects),
        });
        let mut ctx = ExecContext::new(run, reporter, cwd, false);

        let body: [Instruction; 1] = [Instruction::Context {
            project: Template {
                parts: vec![Segment::Cmd(lit("printf app"))],
            },
            body: vec![Instruction::Step {
                row: 0,
                label: "log: inside".to_string(),
                body: vec![],
            }],
        }];
        ctx.exec_stmts(&body).unwrap();

        let updates = updates.lock().unwrap();
        assert_eq!(
            *updates,
            vec![(0, "app".to_string())],
            "the body row is annotated with the resolved name"
        );
    }

    #[test]
    fn test_async_context_is_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        let mut ctx = test_context(BTreeMap::new());
        let before = ctx.cwd.clone();
        let body: [Instruction; 1] = [Instruction::Async {
            body: vec![Instruction::Step {
                row: 0,
                label: "work".to_string(),
                body: vec![Instruction::Cd(lit(other.to_str().unwrap()))],
            }],
        }];
        ctx.exec_stmts(&body).unwrap();
        assert_eq!(
            ctx.cwd, before,
            "a cd inside async must not leak into the enclosing context"
        );
    }
}
