//! Parsed (unresolved) function-body statement types.
//!
//! These are pure syntax: a `FnStmt` is what the parser produces. Resolution
//! into `Instruction` lives in `crate::lower`, so the semantic layer depends
//! on this syntax layer rather than the reverse.

use crate::syntax::source::Template;
use crate::syntax::source::{ArmPattern, EnvPair};

/// A parsed (unresolved) function-body statement.
#[derive(Debug, Clone)]
pub enum FnStmt {
    /// `log (template);` — emit the resolved template to the output log.
    Log(Template),
    /// A binding statement. Created from `var name = (tmpl);`,
    /// `$(cmd) -> name;` (capture), and bare `$(cmd);` (exec statement).
    /// `target == None` means a bare `$(cmd);` exec statement: the resolved
    /// template is run and its stdout logged.
    Bind {
        target: Option<String>,
        value: Template,
    },
    /// `cd (template);` — change the working directory for subsequent commands.
    Cd(Template),
    /// `env { pairs } { body }` — export `pairs` to the command subprocess
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
pub struct Arm {
    pub pattern: ArmPattern,
    pub body: Vec<FnStmt>,
}
