/// A single piece of a [`Template`].
///
/// - `Lit` is literal text emitted verbatim.
/// - `Var` is a `@(name)` reference written by the user. Lowering resolves it
///   to a parameter substitution or a tagged variable reference.
/// - `Cmd` is a `$(command)` substitution: its inner template is resolved to
///   a string, run through `shell -c`, and replaced by its stdout. The inner
///   template may itself contain literals, references, and nested commands.
/// - `Ref` is produced by lowering, never written by a user: one variable
///   declaration's value plus its identity, so the runtime computes the value
///   once and reuses the text. Its template is already fully inlined.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Part {
    Lit(String),
    Var(String),
    Cmd(Template),
    Ref { id: usize, template: Template },
}

/// A template: the single value form in kiru. It is a sequence of parts that
/// resolves to one `String`. Templates are written as `( ... )` and contain
/// `@(name)` data references and `$(command)` substitutions.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Template {
    pub(crate) parts: Vec<Part>,
    pub(crate) offset: usize,
    pub(crate) len: usize,
}

impl Template {
    /// Returns the literal text of the template: literal parts concatenated,
    /// each `@(name)` contributing its name, a variable reference its value.
    /// Commands contribute nothing. Used where a template is expected to be
    /// concrete literal text (case patterns after inlining, test assertions).
    pub(crate) fn literal_text(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Lit(s) => out.push_str(s),
                Part::Var(name) => out.push_str(name),
                Part::Cmd(_) => {}
                Part::Ref { template, .. } => out.push_str(&template.literal_text()),
            }
        }
        out
    }

    /// Whether the template is concrete literal text: no commands, and any
    /// variable reference is itself literal. Checked after inlining, where a
    /// parameter has been substituted and a variable is a tagged reference.
    pub(crate) fn is_literal(&self) -> bool {
        self.parts.iter().all(|part| match part {
            Part::Lit(_) => true,
            Part::Cmd(_) => false,
            Part::Ref { template, .. } => template.is_literal(),
            // Unreachable after inlining; a raw reference is not concrete.
            Part::Var(_) => false,
        })
    }
}

/// A key-value pair for `env` blocks.
#[derive(Debug, Clone)]
pub(crate) struct EnvPair {
    pub(crate) key: String,
    pub(crate) value: Template,
}

/// A pattern arm inside a `switch` block. Patterns are literal `(...)` text
/// (validated after `@()` inlining) or the `default` arm.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ArmPattern {
    /// The `case(...)` pattern template, validated as literal-only at
    /// compile time (after `@()` references are inlined).
    Template(Template),
    Default,
}
