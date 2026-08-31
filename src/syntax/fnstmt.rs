//! Parsed (unresolved) function-body statement types.
//!
//! These are pure syntax: a `FnStmt` is what the parser produces. Resolution
//! into `Instruction` lives in `crate::lower`, so the semantic layer depends
//! on this syntax layer rather than the reverse.

use crate::syntax::source::Template;
use crate::syntax::source::{ArmPattern, EnvPair};

/// A parsed (unresolved) function-body statement.
#[derive(Debug, Clone)]
pub(crate) enum FnStmt {
    /// `log (template);`, emit the resolved template to the output log.
    Log(Template),
    /// A binding statement. Created from `var name = (tmpl);` and bare
    /// `$(cmd);` (exec statement).
    /// `target == None` is always a command template (must contain ≥1 `Cmd`
    /// segment, bare `()` / `@()` as standalone is a parse error). The
    /// resolved template is run strictly (non-zero aborts).
    /// `target == Some(name)` means a variable binding: fully inlined at
    /// compile time, no `Instruction` emitted; execution deferred to use sites.
    Bind {
        target: Option<String>,
        value: Template,
    },
    /// `cd (template);`, change the working directory for subsequent commands.
    Cd(Template),
    /// `env { pairs } { body }`, export `pairs` to the command subprocess
    /// environment for the duration of `body`.
    EnvBlock {
        pairs: Vec<EnvPair>,
        body: Vec<FnStmt>,
    },
    /// `switch (cond) { case (pat) { ... } case _ { ... } }`.
    Switch { subject: Template, arms: Vec<Arm> },
}

/// A single arm of a `switch` block.
#[derive(Debug, Clone)]
pub(crate) struct Arm {
    pub(crate) pattern: ArmPattern,
    pub(crate) body: Vec<FnStmt>,
}
