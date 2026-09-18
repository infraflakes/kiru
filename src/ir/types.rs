//! Type definitions for the execution IR.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single piece of a [`Template`].
///
/// - `Lit` is literal text.
/// - `Cmd` is a `$(command)` substitution whose inner template is run through
///   `shell -c` at runtime and replaced by its captured stdout.
///
/// `@(var)` references no longer exist in the IR: the compiler inlines every
/// variable and function argument into the template that uses it before the
/// IR is built, so there is no runtime scope to resolve against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum Segment {
    Lit(String),
    /// A `$(command)` substitution. The inner template is run through `shell -c`
    /// at runtime.
    Cmd(Template),
}

/// A template: the single string-valued form in the DSL.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct Template {
    pub(crate) parts: Vec<Segment>,
}

impl Template {
    /// A template consisting of a single literal string. Test-only helper.
    #[cfg(test)]
    pub(crate) fn lit(s: &str) -> Self {
        Template {
            parts: vec![Segment::Lit(s.to_string())],
        }
    }
}

/// A single resolved `env` block pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct EnvPair {
    pub(crate) key: String,
    pub(crate) value: Template,
}

/// A pattern arm inside a `switch` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum ArmPattern {
    /// A literal string to match the resolved subject against.
    Lit(String),
    /// The `default` arm.
    Default,
}

/// A single arm of a resolved `switch` block. The arm is a display row of
/// its own (`switch case <pattern>` / `switch default`); untaken arms end
/// as `Skipped` at runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Arm {
    pub(crate) row: usize,
    pub(crate) pattern: ArmPattern,
    pub(crate) body: Vec<Instruction>,
}

/// A fully resolved function-body instruction, ready to execute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum Instruction {
    /// Run the resolved `command` template through `shell -c` with live
    /// output; a non-zero exit aborts. Substitutions (`$()`/`@()`) resolve
    /// first, then the resulting text is the command; variables and
    /// arguments are already inlined away at compile time.
    Exec { command: Template },
    /// Emit `value` to the output log.
    Log(Template),
    /// Change the working directory to the resolved `value`.
    Cd(Template),
    /// Export `pairs` to the command subprocess environment for the duration
    /// of `body`.
    Env {
        pairs: Vec<EnvPair>,
        body: Vec<Instruction>,
    },
    /// Match the resolved `subject` against each arm's pattern; the first
    /// matching arm runs with an isolated local scope frame.
    Switch { subject: Template, arms: Vec<Arm> },
    /// One displayed statement: every primitive and every block statement
    /// is wrapped in a `Step` at compile time, so the TUI shows exactly one
    /// row per statement. Calls are flattened (their inlined statements
    /// become sibling steps). `row` is the display index assigned at
    /// compile time; `label` names the step. A block statement's `body`
    /// holds the block instruction (`Env`/`Async`) whose statements are
    /// indented under this row.
    Step {
        row: usize,
        label: String,
        body: Vec<Instruction>,
    },
    /// Run `body` now and join it at the end of the enclosing body. The
    /// body executes on a copy of the current context (working directory,
    /// environment layers, direnv wrapping), so changes inside never leak
    /// out. A failure anywhere stops the run: sibling processes are killed
    /// and pending steps are cancelled.
    Async { body: Vec<Instruction> },
    /// Execute `body` in a project's context: the directory and direnv
    /// setting from `kiru.toml`. The context is restored when the body
    /// finishes. The project name is a template: `@()` references are
    /// inlined at compile time, any `$()` part resolves at runtime. A
    /// call outside every `Context` runs at the invocation context.
    Context {
        project: Template,
        body: Vec<Instruction>,
    },
}

/// The final, fully resolved IR: every run block as an instruction tree,
/// with calls inlined and arguments bound. Each run's direct statements are
/// `Task` rows; `async` bodies group rows that run concurrently.
///
/// Shell, timeout, and repository configuration live in `kiru.toml` and are
/// injected at execution time by the CLI. The IR is purely behavioral.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct Ir {
    pub(crate) runs: BTreeMap<String, Vec<Instruction>>,
}

/// One rendered line of a run: every step and every switch arm is a line,
/// with its indentation depth, tree prefix, and the project context it
/// executes in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanLine {
    pub(crate) row: usize,
    pub(crate) depth: usize,
    pub(crate) label: String,
    /// The compile-time project annotation: the literal `project(...)` name
    /// when known, or the unresolved template text for runtime-resolved
    /// names. `None` outside every project context.
    pub(crate) project: Option<String>,
    /// Tree-branch prefix for this line (`│  ├─ `), so every renderer draws
    /// the same structure.
    pub(crate) prefix: String,
    /// Prefix for this line's output block (`│     `), hanging the output
    /// under the line.
    pub(crate) output_prefix: String,
}

impl Ir {
    /// The display plan of a run: every step and switch arm in row order,
    /// with indentation and project annotation. This is the single display
    /// source: the renderer, the final dump, and `status` all consume it,
    /// and the row indices match the compile-assigned `Step`/`Arm` rows.
    pub(crate) fn run_plan(&self, run: &str) -> Vec<PlanLine> {
        let Some(body) = self.runs.get(run) else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        collect_plan_lines(body, 0, None, &mut lines);
        assign_tree_prefixes(&mut lines);
        lines
    }

    /// Serialize this IR to the textual kirufile format (RON). The IR is
    /// the run file: RON keeps it human-readable while serde derives keep
    /// the codec in lockstep with these type definitions.
    pub(crate) fn serialize(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .expect("the IR only contains serializable data")
    }

    /// Parse a textual kirufile into an `Ir`.
    pub(crate) fn deserialize(src: &str) -> Result<Ir, String> {
        ron::from_str(src).map_err(|e| e.to_string())
    }
}

/// Walk one body into plan lines. Steps and switch arms are lines; steps
/// whose body opens a block (`env`, `async`) indent their children one
/// level; `switch` statements themselves have no line, only their arms.
/// Project contexts annotate without opening a level.
fn collect_plan_lines(
    body: &[Instruction],
    depth: usize,
    project: Option<&str>,
    lines: &mut Vec<PlanLine>,
) {
    for instruction in body {
        match instruction {
            Instruction::Step { row, label, body } => {
                lines.push(PlanLine {
                    row: *row,
                    depth,
                    label: label.clone(),
                    project: project.map(str::to_string),
                    prefix: String::new(),
                    output_prefix: String::new(),
                });
                for inner in body {
                    match inner {
                        Instruction::Env { body, .. } | Instruction::Async { body } => {
                            collect_plan_lines(body, depth + 1, project, lines);
                        }
                        Instruction::Switch { arms, .. } => {
                            collect_plan_arms(arms, depth + 1, project, lines);
                        }
                        _ => {}
                    }
                }
            }
            Instruction::Switch { arms, .. } => collect_plan_arms(arms, depth, project, lines),
            Instruction::Context {
                project: name,
                body,
            } => {
                let annotation = project_annotation(name);
                collect_plan_lines(body, depth, Some(&annotation), lines);
            }
            _ => {}
        }
    }
}

/// Switch arms are rows at the switch statement's own depth; their bodies
/// indent one level under the arm line.
fn collect_plan_arms(arms: &[Arm], depth: usize, project: Option<&str>, lines: &mut Vec<PlanLine>) {
    for arm in arms {
        let label = match &arm.pattern {
            ArmPattern::Lit(pattern) if pattern.is_empty() => "switch case \"\"".to_string(),
            ArmPattern::Lit(pattern) => format!("switch case {pattern}"),
            ArmPattern::Default => "switch default".to_string(),
        };
        lines.push(PlanLine {
            row: arm.row,
            depth,
            label,
            project: project.map(str::to_string),
            prefix: String::new(),
            output_prefix: String::new(),
        });
        collect_plan_lines(&arm.body, depth + 1, project, lines);
    }
}

/// The plan preview of a template: its literal text with every `$(command)`
/// kept verbatim, because parenthesized content is data, not an operator to
/// summarize. `@()` references are already inlined at compile time.
pub(crate) fn template_plan_text(template: &Template) -> String {
    template
        .parts
        .iter()
        .map(|segment| match segment {
            Segment::Lit(text) => text.clone(),
            Segment::Cmd(inner) => format!("$({})", template_plan_text(inner)),
        })
        .collect()
}

/// The display label of a `log` statement: one definition shared by the
/// compile-time plan and the runtime resolved-label update.
pub(crate) fn log_label(text: &str) -> String {
    format!("log: {text}")
}

/// The display label of an `exec` statement.
pub(crate) fn exec_label(text: &str) -> String {
    format!("exec: {text}")
}

/// The display label of a `cd` statement.
pub(crate) fn cd_label(text: &str) -> String {
    format!("cd: {text}")
}

/// The display label of an `env` block.
pub(crate) fn env_label(keys: &str) -> String {
    format!("env: {keys}")
}

/// The `[project]` annotation for a context name template: the literal name
/// when compile-time known, otherwise the unresolved template text (the IR
/// is portable; shells are not).
fn project_annotation(project: &Template) -> String {
    template_plan_text(project)
}

/// Compute tree-branch prefixes for a flat, pre-order sequence of depths.
///
/// A line is the last child of its parent when the next line at its depth or
/// shallower closes its subtree; ancestor continuation bars come from the
/// open lines above it. One implementation shared by the static plan, the
/// live TUI, and the final dump, so the views never re-derive the structure.
///
/// Returns `(prefix, output_prefix)` per line: the connector-prefixed label
/// ledger and the bar-aligned indent for its output block.
pub(crate) fn tree_prefixes(depths: &[usize]) -> Vec<(String, String)> {
    let count = depths.len();
    let mut is_last = vec![false; count];
    for (index, depth) in depths.iter().enumerate() {
        let mut next = index + 1;
        while next < count && depths[next] > *depth {
            next += 1;
        }
        is_last[index] = next >= count || depths[next] < *depth;
    }

    let mut open: Vec<bool> = Vec::new();
    let mut prefixes = Vec::with_capacity(count);
    for (index, depth) in depths.iter().enumerate() {
        // Close every line whose subtree ended before this one.
        open.truncate(*depth);
        let mut prefix = String::new();
        for last in &open {
            prefix.push_str(if *last { "   " } else { "│  " });
        }
        let output_prefix = format!("{prefix}{}", if is_last[index] { "   " } else { "│  " });
        // The outermost level is the run body itself: no branch there, the
        // statements just list top-down. Deeper levels draw connectors.
        if *depth > 0 {
            prefix.push_str(if is_last[index] { "└─ " } else { "├─ " });
        }
        prefixes.push((prefix, format!("{output_prefix}  ")));
        open.push(is_last[index]);
    }
    prefixes
}

/// Assign the computed tree prefixes to the flat, pre-order plan lines.
fn assign_tree_prefixes(lines: &mut [PlanLine]) {
    let depths: Vec<usize> = lines.iter().map(|line| line.depth).collect();
    for (line, (prefix, output_prefix)) in lines.iter_mut().zip(tree_prefixes(&depths)) {
        line.prefix = prefix;
        line.output_prefix = output_prefix;
    }
}
