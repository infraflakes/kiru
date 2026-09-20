//! The arena interpreter: executes a [`Program`] while writing its runtime
//! state into the shared [`Display`].
//!
//! Working directory and environment layers are the context that `cd` and
//! `env` mutate; `async` bodies fork this state so their mutations never
//! leak out. Every node a body runs writes its own status, output, and
//! resolved values by node id, so execution is the display.

use super::subprocess;
use super::subprocess::RunKillSwitch;
use crate::exec::error::RuntimeError;
use crate::exec::model::{Display, TaskStatus};
use crate::ir::{ArmPattern, EnvPair, Node, NodeId, NodeKind, Program, Segment, Template, VarId};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// One body's execution state. The program is borrowed (it outlives the
/// run); the display is shared with the renderers and with every async body
/// forked from this context.
pub(crate) struct ExecContext<'p> {
    program: &'p Program,
    display: Arc<Mutex<Display>>,
    run: Arc<RunContext>,
    cwd: PathBuf,
    /// Values already computed for tagged variable references. A variable's
    /// commands run the first time its value is needed and never again in
    /// this context; async bodies start from a copy of what is known here.
    var_values: BTreeMap<VarId, String>,
    env_layers: Vec<BTreeMap<String, String>>,
    /// Commands run via `direnv exec <cwd>` when the project opted in with
    /// `direnv = true`. Entering a project context approves the rc first;
    /// direnv itself resolves per-directory rc rules when it runs.
    direnv_wrap: bool,
}

impl<'p> ExecContext<'p> {
    /// Create the root execution context of a run: `cwd` is the invocation
    /// directory and every row starts pending in `display`.
    pub(crate) fn new(
        program: &'p Program,
        display: Arc<Mutex<Display>>,
        run: Arc<RunContext>,
        cwd: PathBuf,
        direnv_wrap: bool,
    ) -> Self {
        ExecContext {
            program,
            display,
            run,
            cwd,
            var_values: BTreeMap::new(),
            env_layers: Vec::new(),
            direnv_wrap,
        }
    }

    /// A copy of this context for one `async` body: same shared program,
    /// display, and run state, but its own working directory, environment
    /// layers, and direnv wrapping. Mutations on the copy never leak back.
    fn fork(&self) -> ExecContext<'p> {
        ExecContext {
            program: self.program,
            display: Arc::clone(&self.display),
            run: Arc::clone(&self.run),
            cwd: self.cwd.clone(),
            var_values: self.var_values.clone(),
            env_layers: self.env_layers.clone(),
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

    /// Update the shared display in one short critical section.
    fn display<R>(&self, update: impl FnOnce(&mut Display) -> R) -> R {
        let mut guard = self.display.lock().unwrap_or_else(|e| e.into_inner());
        update(&mut guard)
    }

    fn set_status(&self, node: NodeId, status: TaskStatus) {
        self.display(|display| display.set_status(node, status));
    }

    fn set_label(&self, node: NodeId, label: String) {
        self.display(|display| display.set_label(node, label));
    }

    fn set_project(&self, node: NodeId, project: String) {
        self.display(|display| display.set_project(node, project));
    }

    fn push_output(&self, node: NodeId, line: String) {
        self.display(|display| display.push_output(node, line));
    }

    /// Resolve a template to a string, tolerantly: `$(cmd)` parts run and
    /// their stdout is captured and inlined, an empty or failed command
    /// contributing nothing. A tagged variable reference computes once,
    /// stores its text in this context, and reuses it for every later use.
    fn resolve(&mut self, tmpl: &Template) -> Result<String, RuntimeError> {
        let mut out = String::new();
        for segment in &tmpl.parts {
            match segment {
                Segment::Lit(s) => out.push_str(s),
                Segment::Cmd(inner) => {
                    let cmd = self.resolve(inner)?;
                    match self.capture(&cmd) {
                        Ok(captured) => out.push_str(&captured),
                        Err(RuntimeError::Timeout { .. }) => {
                            // Tolerant mode: an inner timeout silently
                            // returns an empty string, no propagation.
                        }
                        Err(e) => return Err(e),
                    }
                }
                Segment::Ref(id) => {
                    // The program holds the value once, by identity.
                    let template = self.program.var(*id);
                    let value = match self.var_values.get(id) {
                        Some(value) => value.clone(),
                        None => {
                            let value = self.resolve(template)?;
                            self.var_values.insert(*id, value.clone());
                            value
                        }
                    };
                    out.push_str(&value);
                }
            }
        }
        Ok(out)
    }

    /// Resolve a template and render its failure on the node, so the node
    /// that produced the error is the one that shows it.
    fn resolve_or_report(&mut self, node: NodeId, tmpl: &Template) -> Result<String, RuntimeError> {
        self.resolve(tmpl)
            .inspect_err(|error| self.report_failure(node, error))
    }

    /// Render one failure on its node. Timeouts are skipped: `run_live`
    /// already rendered them.
    fn report_failure(&self, node: NodeId, error: &RuntimeError) {
        if error.is_timeout() {
            return;
        }
        self.push_output(node, format!("Error: {error}"));
    }

    /// Run a command and stream its output live into `node` (fatal on
    /// non-zero exit or timeout).
    fn run_live(&self, node: NodeId, cmd: &str) -> Result<(), RuntimeError> {
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
                    self.push_output(node, text);
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
                self.push_output(
                    node,
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
    /// `Err(Timeout { .. })` when the process exceeds the global timeout.
    /// Non-zero exit is non-fatal.
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
        // Shell value: trailing newlines are not part of what the command
        // substituted; every other byte is.
        .map(|text| text.trim_end_matches('\n').to_string())
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
            for (key, value) in layer {
                env.insert(key.clone(), value.clone());
            }
        }
        env
    }

    /// Execute one body and join every `async` it started when the body
    /// ends (structured concurrency). This is the single execution entry
    /// point: run bodies, `env`/`async`/project bodies, and switch arms all
    /// recurse here.
    ///
    /// Fail-fast: the first genuine failure marks the run failed (killing
    /// every live command group) and marks the pending siblings cancelled.
    pub(crate) fn exec_children(&mut self, children: &[NodeId]) -> Result<(), RuntimeError> {
        let mut child_failure: Option<RuntimeError> = None;
        let sequence = std::thread::scope(|scope| {
            let mut pending: Vec<(
                NodeId,
                std::thread::ScopedJoinHandle<'_, Result<(), RuntimeError>>,
            )> = Vec::new();
            let result = self.exec_sequence(children, scope, &mut pending);

            // Structured concurrency: this body is not finished until every
            // async body it started is finished.
            for (node, handle) in pending {
                match handle.join() {
                    Ok(Ok(())) => self.set_status(node, TaskStatus::Success),
                    Ok(Err(error)) => {
                        self.set_status(node, failure_status(&error));
                        if child_failure.is_none() {
                            child_failure = Some(error);
                        }
                    }
                    Err(_) => {
                        self.set_status(node, TaskStatus::Error);
                        if child_failure.is_none() {
                            child_failure =
                                Some(RuntimeError::exec_io_error("async", "task panicked"));
                        }
                    }
                }
            }
            result
        });

        match (sequence, child_failure) {
            (Err(error), _) => Err(error),
            (Ok(()), Some(error)) => Err(error),
            (Ok(()), None) => Ok(()),
        }
    }

    /// The sequential statement loop of one body. Async children are spawned
    /// into `pending` and joined by [`Self::exec_children`] after it.
    fn exec_sequence<'s>(
        &mut self,
        children: &[NodeId],
        scope: &'s std::thread::Scope<'s, '_>,
        pending: &mut Vec<(
            NodeId,
            std::thread::ScopedJoinHandle<'s, Result<(), RuntimeError>>,
        )>,
    ) -> Result<(), RuntimeError>
    where
        'p: 's,
    {
        for (index, &node) in children.iter().enumerate() {
            if self.run_failed() {
                self.mark_subtree(&children[index..], TaskStatus::Cancelled);
                return Err(RuntimeError::Cancelled(
                    "run failed in another task".to_string(),
                ));
            }
            // Every node a body starts reports Running centrally, so leaf
            // commands show as live work just like blocks do.
            self.set_status(node, TaskStatus::Running);
            let result = self.exec_node(node, scope, pending);
            if let Err(error) = result {
                if !matches!(error, RuntimeError::Cancelled(_))
                    && let Some(kill) = &self.run.kill
                {
                    kill.fail();
                }
                self.mark_subtree(&children[index + 1..], TaskStatus::Cancelled);
                return Err(error);
            }
        }
        Ok(())
    }

    /// Execute one node, dispatching on its kind. Structural nodes
    /// (`switch`, project) recurse without having a row of their own; a
    /// `switch` marks the arms it did not take skipped before running the
    /// taken one.
    fn exec_node<'s>(
        &mut self,
        node: NodeId,
        scope: &'s std::thread::Scope<'s, '_>,
        pending: &mut Vec<(
            NodeId,
            std::thread::ScopedJoinHandle<'s, Result<(), RuntimeError>>,
        )>,
    ) -> Result<(), RuntimeError>
    where
        // The forked async body borrows the program, so the program must
        // outlive the thread scope that runs it.
        'p: 's,
    {
        let program = self.program;
        let Node { kind, children } = program.node(node);
        match kind {
            NodeKind::Log(template) => {
                let result = if template.is_dynamic(&self.program.vars) {
                    match self.resolve_or_report(node, template) {
                        Ok(resolved) => {
                            if let Some(label) = kind.resolved_label(&resolved) {
                                self.set_label(node, label);
                            }
                            Ok(())
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    Ok(())
                };
                self.finish(node, result)
            }
            NodeKind::Exec(template) => {
                let result = match self.resolve_or_report(node, template) {
                    Ok(cmd) => {
                        if template.is_dynamic(&self.program.vars)
                            && let Some(label) = kind.resolved_label(&cmd)
                        {
                            self.set_label(node, label);
                        }
                        self.run_live(node, &cmd)
                            .inspect_err(|error| self.report_failure(node, error))
                    }
                    Err(error) => Err(error),
                };
                self.finish(node, result)
            }
            NodeKind::Cd(template) => {
                let result = match self.resolve_or_report(node, template) {
                    Ok(target) => {
                        match self
                            .exec_cd(&target)
                            .inspect_err(|error| self.report_failure(node, error))
                        {
                            Ok(()) => {
                                if template.is_dynamic(&self.program.vars)
                                    && let Some(label) = kind.resolved_label(&target)
                                {
                                    self.set_label(node, label);
                                }
                                Ok(())
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                };
                self.finish(node, result)
            }
            NodeKind::Env(pairs) => {
                let result = self.exec_env_block(node, pairs, children);
                self.finish(node, result)
            }
            NodeKind::Async => {
                let mut body = self.fork();
                let body_children = children.to_vec();
                pending.push((
                    node,
                    scope.spawn(move || body.exec_children(&body_children)),
                ));
                Ok(())
            }
            NodeKind::Switch(subject) => self.exec_switch(node, subject, children),
            NodeKind::Arm(_) => self.exec_arm(node),
            NodeKind::Project(project) => self.exec_project(node, project, children),
        }
    }

    /// Run the `env` pairs' targets, then the block body with the extra
    /// environment in effect. Only pair resolution failures are rendered
    /// here; body failures already rendered on their own rows.
    fn exec_env_block(
        &mut self,
        node: NodeId,
        pairs: &[EnvPair],
        children: &[NodeId],
    ) -> Result<(), RuntimeError> {
        let mut layer = BTreeMap::new();
        for pair in pairs {
            let value = self.resolve_or_report(node, &pair.value)?;
            layer.insert(pair.key.clone(), value);
        }
        self.env_layers.push(layer);
        let result = self.exec_children(children);
        self.env_layers.pop();
        result
    }

    /// Match the resolved subject against each arm and run the first match.
    /// Every other arm ends terminal as skipped, so the display stops
    /// waiting for it.
    fn exec_switch(
        &mut self,
        node: NodeId,
        subject: &Template,
        arms: &[NodeId],
    ) -> Result<(), RuntimeError> {
        let condition = match self.resolve_or_report(node, subject) {
            Ok(condition) => condition,
            Err(error) => {
                self.set_status(node, TaskStatus::Error);
                return Err(error);
            }
        };
        let program = self.program;
        let taken = arms.iter().position(|&arm| match &program.node(arm).kind {
            NodeKind::Arm(ArmPattern::Lit(pattern)) => pattern == &condition,
            NodeKind::Arm(ArmPattern::Default) => true,
            _ => false,
        });
        for (index, &arm) in arms.iter().enumerate() {
            if Some(index) != taken {
                self.mark_subtree(&[arm], TaskStatus::Skipped);
            }
        }
        let result = match taken {
            Some(index) => self.exec_arm(arms[index]),
            None => Ok(()),
        };
        self.set_status(node, failure_status_or_success(&result));
        result
    }

    /// Run one switch arm's children, with the arm node carrying the
    /// outcome.
    fn exec_arm(&mut self, arm: NodeId) -> Result<(), RuntimeError> {
        self.set_status(arm, TaskStatus::Running);
        let children = self.program.node(arm).children.clone();
        let result = self.exec_children(&children);
        self.finish(arm, result)
    }

    /// Enter a project context, run its children there, and restore the
    /// previous context afterwards. A failure to enter the context is
    /// rendered on the first row of the body; body failures carry their own
    /// rows and are not re-reported.
    fn exec_project(
        &mut self,
        node: NodeId,
        project: &Template,
        children: &[NodeId],
    ) -> Result<(), RuntimeError> {
        let (name, entry) = match self.enter_context(project) {
            Ok(pair) => pair,
            Err(error) => {
                self.report_context_failure(children, &error);
                self.set_status(node, TaskStatus::Error);
                return Err(error);
            }
        };
        // A dynamic project name is only known here, so every row of the
        // body is annotated with the resolved name it actually ran under.
        if project.is_dynamic(&self.program.vars) {
            let program = self.program;
            for &child in children {
                let name = name.clone();
                program.visit_subtree(&[child], &mut |id| self.set_project(id, name.clone()));
            }
        }
        let previous_cwd = std::mem::replace(&mut self.cwd, entry.dir);
        let previous_wrap = self.direnv_wrap;
        self.direnv_wrap = entry.direnv;
        let result = self.exec_children(children);
        self.cwd = previous_cwd;
        self.direnv_wrap = previous_wrap;
        self.finish(node, result)
    }

    /// Set `node`'s terminal status from its own outcome.
    fn finish(&self, node: NodeId, result: Result<(), RuntimeError>) -> Result<(), RuntimeError> {
        self.set_status(node, failure_status_or_success(&result));
        result
    }

    /// Resolve the project name and approve its direnv environment, without
    /// entering the directory yet. Returns the resolved name with its entry.
    fn enter_context(&mut self, project: &Template) -> Result<(String, ProjectExec), RuntimeError> {
        let name = self.resolve(project)?;
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

    /// Mark every node reachable from `roots` with `status`.
    fn mark_subtree(&self, roots: &[NodeId], status: TaskStatus) {
        let program = self.program;
        program.visit_subtree(roots, &mut |id| self.set_status(id, status));
    }

    /// Report a failure to enter a project context: there is no row for the
    /// context itself, so the first row of its body carries the error and
    /// every other row is cancelled before the run stops.
    fn report_context_failure(&self, roots: &[NodeId], error: &RuntimeError) {
        let program = self.program;
        let mut first: Option<NodeId> = None;
        for &root in roots {
            program.visit_subtree(&[root], &mut |id| {
                if first.is_none() && program.node(id).kind.row_label(&program.vars).is_some() {
                    first = Some(id);
                }
            });
        }
        let Some(first) = first else {
            return;
        };
        self.set_status(first, TaskStatus::Error);
        self.push_output(first, format!("Error: {error}"));
        for &root in roots {
            program.visit_subtree(&[root], &mut |id| {
                if id != first {
                    self.set_status(id, TaskStatus::Cancelled);
                }
            });
        }
    }
}

/// The terminal status of a finished outcome.
fn failure_status(error: &RuntimeError) -> TaskStatus {
    match error {
        RuntimeError::Cancelled(_) => TaskStatus::Cancelled,
        _ => TaskStatus::Error,
    }
}

/// The terminal status of a finished outcome: success when it is `Ok`.
fn failure_status_or_success(result: &Result<(), RuntimeError>) -> TaskStatus {
    match result {
        Ok(()) => TaskStatus::Success,
        Err(error) => failure_status(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ArmPattern, NodeKind, ProgramBuilder};
    use std::collections::BTreeMap;
    use std::time::Duration;

    fn lit(s: &str) -> Template {
        Template {
            parts: vec![Segment::Lit(s.to_string())],
        }
    }

    fn cmd(inner: &str) -> Template {
        Template {
            parts: vec![Segment::Cmd(lit(inner))],
        }
    }

    fn run_context(projects: BTreeMap<String, ProjectExec>) -> Arc<RunContext> {
        Arc::new(RunContext {
            shell: "sh".to_string(),
            timeout: Some(Duration::from_secs(30)),
            kill: None,
            projects: Arc::new(projects),
        })
    }

    fn test_context<'p>(
        program: &'p Program,
        projects: BTreeMap<String, ProjectExec>,
    ) -> (ExecContext<'p>, Arc<Mutex<Display>>) {
        let display = Arc::new(Mutex::new(Display::for_program(program)));
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let ctx = ExecContext::new(
            program,
            Arc::clone(&display),
            run_context(projects),
            cwd,
            false,
        );
        (ctx, display)
    }

    fn status(display: &Arc<Mutex<Display>>, id: NodeId) -> TaskStatus {
        display.lock().unwrap().state(id).status
    }

    fn outputs(display: &Arc<Mutex<Display>>, id: NodeId) -> Vec<String> {
        display.lock().unwrap().state(id).output.clone()
    }

    #[test]
    fn test_switch_first_match() {
        let mut build = ProgramBuilder::default();
        let log = build.push(NodeKind::Log(lit("first")), vec![]);
        let taken = build.push(NodeKind::Arm(ArmPattern::Lit("a".to_string())), vec![log]);
        let default = build.push(NodeKind::Arm(ArmPattern::Default), vec![]);
        let switch = build.push(NodeKind::Switch(lit("a")), vec![taken, default]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![switch])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        assert_eq!(status(&display, taken), TaskStatus::Success);
        assert_eq!(status(&display, log), TaskStatus::Success);
        assert_eq!(status(&display, default), TaskStatus::Skipped);
    }

    #[test]
    fn test_async_runs_and_joins() {
        let mut build = ProgramBuilder::default();
        let log = build.push(NodeKind::Log(lit("async ran")), vec![]);
        let group = build.push(NodeKind::Async, vec![log]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![group])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        assert_eq!(status(&display, group), TaskStatus::Success);
        assert_eq!(status(&display, log), TaskStatus::Success);
    }

    #[test]
    fn test_context_unknown_project_errors() {
        let mut build = ProgramBuilder::default();
        let project = build.push(NodeKind::Project(lit("missing")), vec![]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![project])]));

        let (mut ctx, _display) = test_context(&program, BTreeMap::new());
        let error = ctx.exec_children(program.run_children("r")).unwrap_err();
        assert!(error.to_string().contains("unknown project"), "{error}");
    }

    #[test]
    fn test_context_restores_cwd() {
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
        let mut build = ProgramBuilder::default();
        let cd = build.push(NodeKind::Cd(lit(".")), vec![]);
        let project = build.push(NodeKind::Project(lit("p")), vec![cd]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![project])]));

        let (mut ctx, _display) = test_context(&program, projects);
        ctx.exec_children(program.run_children("r")).unwrap();
        let invocation = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        assert_eq!(ctx.cwd, invocation, "context cwd must not leak out");
    }

    #[test]
    fn test_context_failure_is_reported_on_its_first_row() {
        let mut build = ProgramBuilder::default();
        let first = build.push(NodeKind::Log(lit("a")), vec![]);
        let second = build.push(NodeKind::Log(lit("b")), vec![]);
        let project = build.push(NodeKind::Project(lit("missing")), vec![first, second]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![project])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        let error = ctx.exec_children(program.run_children("r")).unwrap_err();
        assert!(error.to_string().contains("unknown project"), "{error}");
        assert_eq!(status(&display, first), TaskStatus::Error);
        assert_eq!(status(&display, second), TaskStatus::Cancelled);
        assert!(
            outputs(&display, first)
                .iter()
                .any(|line| line.contains("unknown project")),
            "the error is rendered on the first row"
        );
    }

    #[test]
    fn test_untaken_switch_arms_are_skipped() {
        let mut build = ProgramBuilder::default();
        let taken_log = build.push(NodeKind::Log(lit("taken")), vec![]);
        let taken = build.push(
            NodeKind::Arm(ArmPattern::Lit("a".to_string())),
            vec![taken_log],
        );
        let untaken_log = build.push(NodeKind::Log(lit("untaken")), vec![]);
        let untaken = build.push(NodeKind::Arm(ArmPattern::Default), vec![untaken_log]);
        let switch = build.push(NodeKind::Switch(lit("a")), vec![taken, untaken]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![switch])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        assert_eq!(status(&display, taken), TaskStatus::Success);
        assert_eq!(status(&display, untaken), TaskStatus::Skipped);
        assert_eq!(status(&display, untaken_log), TaskStatus::Skipped);
        assert_ne!(
            status(&display, untaken_log),
            TaskStatus::Running,
            "skipped steps never run"
        );
    }

    /// Parenthesized content is data: a dynamic template resolves when it
    /// executes and replaces the node's plan label with the data.
    #[test]
    fn test_dynamic_labels_resolve_at_runtime() {
        let mut build = ProgramBuilder::default();
        let exec = build.push(
            NodeKind::Exec(Template {
                parts: vec![
                    Segment::Lit("echo ".to_string()),
                    Segment::Cmd(lit("printf hi")),
                ],
            }),
            vec![],
        );
        let log = build.push(NodeKind::Log(cmd("printf hi")), vec![]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![exec, log])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        let guard = display.lock().unwrap();
        assert_eq!(
            guard.state(exec).label.as_deref(),
            Some("exec: echo hi"),
            "the executed command renames its node"
        );
        assert_eq!(
            guard.state(log).label.as_deref(),
            Some("log: hi"),
            "the logged data renames its node"
        );
        assert!(
            guard.state(log).output.is_empty(),
            "a resolved log is its label, not an output line"
        );
    }

    /// A dynamic project name is only known at entry; every row of the body
    /// is annotated with the name it actually ran under.
    #[test]
    fn test_dynamic_project_name_annotates_body_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut projects = BTreeMap::new();
        projects.insert(
            "app".to_string(),
            ProjectExec {
                dir: dir.path().to_path_buf(),
                direnv: false,
            },
        );
        let mut build = ProgramBuilder::default();
        let log = build.push(NodeKind::Log(lit("inside")), vec![]);
        let project = build.push(NodeKind::Project(cmd("printf app")), vec![log]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![project])]));

        let (mut ctx, display) = test_context(&program, projects);
        ctx.exec_children(program.run_children("r")).unwrap();

        let guard = display.lock().unwrap();
        assert_eq!(
            guard.state(log).project.as_deref(),
            Some("app"),
            "the body row is annotated with the resolved name"
        );
    }

    /// A failure renders once, on the node that produced it: enclosing
    /// blocks only change status.
    #[test]
    fn test_failure_renders_once_on_its_own_row() {
        let mut build = ProgramBuilder::default();
        let exec = build.push(NodeKind::Exec(lit("exit 7")), vec![]);
        let env = build.push(NodeKind::Env(Vec::new()), vec![exec]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![env])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap_err();

        let error_lines: Vec<(NodeId, String)> = display
            .lock()
            .unwrap()
            .states()
            .iter()
            .enumerate()
            .flat_map(|(id, state)| {
                state
                    .output
                    .iter()
                    .filter(|line| line.starts_with("Error:"))
                    .map(move |line| (id, line.clone()))
            })
            .collect();
        assert_eq!(
            error_lines.len(),
            1,
            "exactly one error line: {error_lines:?}"
        );
        assert_eq!(error_lines[0].0, exec, "on the node that failed");
        assert_eq!(status(&display, exec), TaskStatus::Error);
        assert_eq!(status(&display, env), TaskStatus::Error);
        assert!(
            outputs(&display, env).is_empty(),
            "the enclosing block does not repeat the error"
        );
    }

    /// A started leaf command reports Running while it executes, so the
    /// live view can show it as active work.
    #[test]
    fn test_started_leaf_reports_running() {
        let mut build = ProgramBuilder::default();
        let exec = build.push(NodeKind::Exec(lit("sleep 1")), vec![]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![exec])]));
        let display = Arc::new(Mutex::new(Display::for_program(&program)));
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let mut ctx = ExecContext::new(
            &program,
            Arc::clone(&display),
            run_context(BTreeMap::new()),
            cwd,
            false,
        );

        let observed_running = std::thread::scope(|scope| {
            let handle = scope.spawn(|| ctx.exec_children(program.run_children("r")));
            let mut observed_running = false;
            for _ in 0..400 {
                if status(&display, exec) == TaskStatus::Running {
                    observed_running = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            handle.join().expect("context thread").unwrap();
            observed_running
        });

        assert!(observed_running, "a running leaf must report Running");
        assert_eq!(status(&display, exec), TaskStatus::Success);
    }

    /// Store a variable whose first resolution appends one line to
    /// `counter` and yields the text `value`; returns its identity.
    fn counter_var(build: &mut ProgramBuilder, counter: &std::path::Path) -> VarId {
        build.push_var(Template {
            parts: vec![Segment::Cmd(lit(&format!(
                "echo hit >> '{}'; echo value",
                counter.display()
            )))],
        })
    }

    fn counter_lines(counter: &std::path::Path) -> usize {
        std::fs::read_to_string(counter).map_or(0, |text| text.lines().count())
    }

    /// A variable runs its command once per context and reuses the text for
    /// every later use.
    #[test]
    fn test_variable_value_is_computed_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("count");
        let mut build = ProgramBuilder::default();
        let stamp = counter_var(&mut build, &counter);
        let log = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Ref(stamp), Segment::Ref(stamp)],
            }),
            vec![],
        );
        let program = build.build(BTreeMap::from([("r".to_string(), vec![log])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        assert_eq!(counter_lines(&counter), 1, "the command ran once");
        assert_eq!(
            display.lock().unwrap().state(log).label.as_deref(),
            Some("log: valuevalue"),
            "both uses see the same computed text"
        );
    }

    /// A value computed before a project entry is reused inside it.
    #[test]
    fn test_variable_value_survives_a_project_entry() {
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("count");
        let project_dir = dir.path().join("proj");
        std::fs::create_dir_all(&project_dir).unwrap();
        let mut projects = BTreeMap::new();
        projects.insert(
            "p".to_string(),
            ProjectExec {
                dir: project_dir,
                direnv: false,
            },
        );

        let mut build = ProgramBuilder::default();
        let stamp = counter_var(&mut build, &counter);
        let before = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Ref(stamp)],
            }),
            vec![],
        );
        let inside = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Ref(stamp)],
            }),
            vec![],
        );
        let project = build.push(NodeKind::Project(lit("p")), vec![inside]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![before, project])]));

        let (mut ctx, display) = test_context(&program, projects);
        ctx.exec_children(program.run_children("r")).unwrap();

        assert_eq!(
            counter_lines(&counter),
            1,
            "computed before the project, reused inside"
        );
        let guard = display.lock().unwrap();
        assert_eq!(guard.state(before).label.as_deref(), Some("log: value"));
        assert_eq!(guard.state(inside).label.as_deref(), Some("log: value"));
    }

    /// A value computed before an async starts is reused inside it.
    #[test]
    fn test_variable_value_is_reused_across_async_copy() {
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("count");
        let mut build = ProgramBuilder::default();
        let stamp = counter_var(&mut build, &counter);
        let inside = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Ref(stamp)],
            }),
            vec![],
        );
        let group = build.push(NodeKind::Async, vec![inside]);
        let before = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Ref(stamp)],
            }),
            vec![],
        );
        let program = build.build(BTreeMap::from([("r".to_string(), vec![before, group])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        assert_eq!(
            counter_lines(&counter),
            1,
            "computed once, reused in the copy"
        );
        assert_eq!(
            display.lock().unwrap().state(inside).label.as_deref(),
            Some("log: value")
        );
    }

    /// A variable first used inside async bodies is computed in each copy:
    /// once per async that needs it, with no shared state between them.
    #[test]
    fn test_variable_first_used_in_each_async_is_computed_there() {
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("count");
        let mut build = ProgramBuilder::default();
        let stamp = counter_var(&mut build, &counter);
        let in_a = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Ref(stamp)],
            }),
            vec![],
        );
        let group_a = build.push(NodeKind::Async, vec![in_a]);
        let in_b = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Ref(stamp)],
            }),
            vec![],
        );
        let group_b = build.push(NodeKind::Async, vec![in_b]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![group_a, group_b])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        assert_eq!(
            counter_lines(&counter),
            2,
            "each async copy computes its own value"
        );
        let guard = display.lock().unwrap();
        assert_eq!(guard.state(in_a).label.as_deref(), Some("log: value"));
        assert_eq!(guard.state(in_b).label.as_deref(), Some("log: value"));
    }

    /// Command substitution keeps everything but trailing newlines.
    #[test]
    fn test_capture_keeps_spaces_and_drops_newlines() {
        let mut build = ProgramBuilder::default();
        let spaces = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Cmd(lit("printf '  '"))],
            }),
            vec![],
        );
        let trailing = build.push(
            NodeKind::Log(Template {
                parts: vec![Segment::Cmd(lit("printf 'a\n\n'"))],
            }),
            vec![],
        );
        let program = build.build(BTreeMap::from([("r".to_string(), vec![spaces, trailing])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        let guard = display.lock().unwrap();
        assert_eq!(guard.state(spaces).label.as_deref(), Some("log:   "));
        assert_eq!(guard.state(trailing).label.as_deref(), Some("log: a"));
    }

    /// Printed output keeps its leading whitespace.
    #[test]
    fn test_output_keeps_leading_whitespace() {
        let mut build = ProgramBuilder::default();
        let exec = build.push(NodeKind::Exec(lit("echo '   indented'")), vec![]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![exec])]));

        let (mut ctx, display) = test_context(&program, BTreeMap::new());
        ctx.exec_children(program.run_children("r")).unwrap();

        assert_eq!(
            outputs(&display, exec),
            vec!["   indented".to_string()],
            "indentation survives"
        );
    }

    #[test]
    fn test_async_context_is_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        let mut build = ProgramBuilder::default();
        let cd = build.push(NodeKind::Cd(lit(other.to_str().unwrap())), vec![]);
        let group = build.push(NodeKind::Async, vec![cd]);
        let program = build.build(BTreeMap::from([("r".to_string(), vec![group])]));

        let (mut ctx, _display) = test_context(&program, BTreeMap::new());
        let before = ctx.cwd.clone();
        ctx.exec_children(program.run_children("r")).unwrap();
        assert_eq!(
            ctx.cwd, before,
            "a cd inside async must not leak into the enclosing context"
        );
    }
}
