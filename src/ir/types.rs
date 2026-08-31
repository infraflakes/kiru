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
pub enum Segment {
    Literal(String),
    /// A `$(command)` substitution. The inner template is run through `shell -c`
    /// at runtime.
    Command(Template),
}

/// A template: the single string-valued form in the DSL.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Template {
    pub segments: Vec<Segment>,
}

impl Template {
    /// A template consisting of a single literal string. Test-only helper used
    /// while building round-trip fixtures.
    #[cfg(test)]
    pub fn lit(s: &str) -> Self {
        Template {
            segments: vec![Segment::Literal(s.to_string())],
        }
    }
}

/// A single resolved `env` block pair.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvPair {
    pub key: String,
    pub value: Template,
}

/// A pattern arm inside a `switch` block.
#[derive(Debug, Clone, PartialEq)]
pub enum ArmPattern {
    /// A literal string to match the resolved subject against.
    Lit(String),
    /// The `_` default arm.
    Default,
}

/// A single arm of a resolved `switch` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub pattern: ArmPattern,
    pub body: Vec<Instruction>,
}

/// A fully resolved function-body instruction, ready to execute.
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    /// Execute `value` for its side effects at runtime. Every `$(command)` part
    /// of the template is run through `shell -c` and streamed to output. There is
    /// no `target`: variable bindings are inlined away at compile time, so an
    /// `exec` is purely a command execution statement.
    Exec { value: Template },
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
pub struct Call {
    pub project: String,
    pub function: String,
}

impl Call {
    /// Fully-qualified `project::function` name used in labels and rendering.
    pub fn fqn(&self) -> String {
        format!("{}::{}", self.project, self.function)
    }
}

/// A resolved repository/sync declaration.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sync {
    pub url: Template,
    pub dir: Template,
    pub branch: Template,
    pub strategy: Template,
}

/// A fully compiled project: its functions (variables are inlined into the
/// templates that use them at compile time, so nothing static lives here).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Project {
    /// Functions belonging to this project, each lowered to `Instruction`s.
    pub functions: BTreeMap<String, Vec<Instruction>>,
}

/// The final, fully resolved IR. The executor works exclusively with this type.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ir {
    /// Shell used for `$(command)` substitution and `exec` statements.
    pub shell: String,
    /// Global timeout in seconds for every `$(cmd)` substitution.
    pub timeout: u64,
    /// Repositories declared via `sync name { ... }`.
    pub repositories: BTreeMap<String, Sync>,
    /// Projects (the merge of a `sync` block and a `pr` block of the same name).
    pub projects: BTreeMap<String, Project>,
    /// Run blocks keyed by name. Each block is an ordered list of chains; calls
    /// joined by `=>` form one sequential chain (each runs after the previous),
    /// and `;` separates chains which run concurrently with one another.
    pub execution_chains: BTreeMap<String, Vec<Vec<Call>>>,
}
