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
    /// A bare `$(cmd);` statement. Must contain at least one `Cmd` segment
    /// (bare `()` / `@()` as a standalone statement is a parse error). The
    /// resolved template is run strictly (non-zero aborts).
    RunShellCmd(Template),
    /// `cd (template);`, change the working directory for subsequent commands.
    Cd(Template),
    /// `env { pairs } { body }`, export `pairs` to the command subprocess
    /// environment for the duration of `body`.
    EnvBlock {
        pairs: Vec<EnvPair>,
        body: Vec<FnStmt>,
    },
    /// `switch cond { case (pat) { ... } case _ { ... } }` with the subject
    /// written directly (template or bare identifier).
    Switch { subject: Template, arms: Vec<Arm> },
}

/// A single arm of a `switch` block.
#[derive(Debug, Clone)]
pub(crate) struct Arm {
    pub(crate) pattern: ArmPattern,
    pub(crate) body: Vec<FnStmt>,
}
