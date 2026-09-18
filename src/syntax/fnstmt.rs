//! Parsed (unresolved) function-body statement types.
//!
//! These are pure syntax: a `FnStmt` is what the parser produces. Resolution
//! into `Instruction` lives in `crate::compile`, so the semantic layer depends
//! on this syntax layer rather than the reverse.

use crate::syntax::source::Template;
use crate::syntax::source::{ArmPattern, EnvPair};

/// A parsed (unresolved) function-body statement.
#[derive(Debug, Clone)]
pub(crate) enum FnStmt {
    /// `log (template);`, emit the resolved template to the output log.
    Log(Template),
    /// `var name = template;` — the value is fully inlined into scope at
    /// compile time; no `Instruction` is emitted and execution is deferred
    /// to each use site.
    Bind { name: String, value: Template },
    /// `exec(cmd);` - run the resolved template as a shell command with
    /// live output (non-zero exit aborts). `$()`/`@()` inside the argument
    /// are substituted first; the resulting text is the command.
    Exec(Template),
    /// `cd (template);`, change the working directory for subsequent commands.
    Cd(Template),
    /// `env { pairs } { body }`, export `pairs` to the command subprocess
    /// environment for the duration of `body`.
    EnvBlock {
        pairs: Vec<EnvPair>,
        body: Vec<FnStmt>,
    },
    /// `switch cond { case (pat) { ... } default() { ... } }` with the subject
    /// written inside the call parens.
    Switch { subject: Template, arms: Vec<Arm> },
    /// `name(args);` - a call to another function. Arguments are positional
    /// templates bound to the callee's params at compile time; the call runs
    /// in the context of the innermost enclosing `project(...)` block, or at
    /// the invocation context.
    Call {
        name: String,
        args: Vec<Template>,
        offset: usize,
        len: usize,
    },
    /// `project(name) { ... }` - execute the body in a project's context:
    /// the directory and direnv setting from `kiru.toml`, restored when the
    /// body finishes. The name is a template, so it may resolve at runtime.
    Project { name: Template, body: Vec<FnStmt> },
    /// `async() { ... }` - start the body now and join it at the end of the
    /// enclosing body (structured concurrency). The body runs on a copy of
    /// the current context, so changes inside it never leak out.
    Async { body: Vec<FnStmt> },
}

/// A single arm of a `switch` block.
#[derive(Debug, Clone)]
pub(crate) struct Arm {
    pub(crate) pattern: ArmPattern,
    pub(crate) body: Vec<FnStmt>,
}
