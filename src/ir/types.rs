//! Type definitions for the execution IR.

use std::collections::BTreeMap;

/// A single piece of a [`Template`].
///
/// - `Literal` is literal text.
/// - `Command` is a `$(command)` substitution whose inner template is run through
///   `shell -c` at runtime and replaced by its captured stdout.
///
/// `@(var)` references no longer exist in the IR: the compiler inlines every
/// variable into the template that uses it before the IR is built, so there is
/// no runtime variable scope to resolve against.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Segment {
    Literal(String),
    /// A `$(command)` substitution. The inner template is run through `shell -c`
    /// at runtime.
    Command(Template),
}

/// A template: the single string-valued form in the DSL.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Template {
    pub(crate) segments: Vec<Segment>,
}

impl Template {
    /// A template consisting of a single literal string. Test-only helper.
    #[cfg(test)]
    pub(crate) fn lit(s: &str) -> Self {
        Template {
            segments: vec![Segment::Literal(s.to_string())],
        }
    }
}

/// A single resolved `env` block pair.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EnvPair {
    pub(crate) key: String,
    pub(crate) value: Template,
}

/// A pattern arm inside a `switch` block.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ArmPattern {
    /// A literal string to match the resolved subject against.
    Lit(String),
    /// The `_` default arm.
    Default,
}

/// A single arm of a resolved `switch` block.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Arm {
    pub(crate) pattern: ArmPattern,
    pub(crate) body: Vec<Instruction>,
}

/// A fully resolved function-body instruction, ready to execute.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Instruction {
    /// Execute `value` for its side effects at runtime. Every `$(command)` part
    /// of the template is run through `shell -c` and streamed to output. There is
    /// no `target`: variable bindings are inlined away at compile time, so a
    /// `RunShellCmd` is purely a command execution statement.
    RunShellCmd { value: Template },
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
}

/// A `project::function` reference inside a `run` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    pub(crate) project: String,
    pub(crate) function: String,
}

impl Call {
    /// Fully-qualified `project::function` name used in labels and rendering.
    pub(crate) fn fqn(&self) -> String {
        format!("{}::{}", self.project, self.function)
    }
}

/// A fully compiled project: its functions (variables are inlined into the
/// templates that use them at compile time, so nothing static lives here).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Project {
    pub(crate) functions: BTreeMap<String, Vec<Instruction>>,
}

/// The final, fully resolved IR. The executor works exclusively with this type.
///
/// Shell, timeout, and repository configuration live in `kiru.toml` and are
/// injected at execution time by the CLI. The IR is purely behavioral: projects
/// (functions) and execution chains (run blocks).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Ir {
    pub(crate) projects: BTreeMap<String, Project>,
    pub(crate) execution_chains: BTreeMap<String, Vec<Vec<Call>>>,
}
