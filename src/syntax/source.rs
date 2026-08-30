/// A single piece of a [`Template`].
///
/// - `Lit` is literal text emitted verbatim.
/// - `Var` is a `@(name)` data reference resolved against the runtime scope
///   stack (local -> project -> global). There is no namespace qualifier.
/// - `Cmd` is a `$(command)` substitution. Its inner template is data-only
///   (literal / var, never a nested `Cmd` in well-formed input) and is resolved
///   to a string, run through `shell -c`, and replaced by its stdout.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Lit(String),
    Var(String),
    Cmd(Template),
}

impl Default for Part {
    fn default() -> Self {
        Part::Lit(String::new())
    }
}

/// A template: the single value form in kiru. It is a sequence of parts that
/// resolves to one `String`. Backtick strings and `@{ns::name}` interpolation
/// are gone; templates are written as `( ... )` and contain `@(name)` data
/// references and `$(command)` substitutions.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Template {
    pub parts: Vec<Part>,
    pub offset: usize,
    pub len: usize,
    /// Canonical path of the `.kiru` file this template was parsed from.
    pub source_name: String,
}

impl Template {
    /// A template consisting of a single literal string.
    pub fn lit(s: &str) -> Template {
        Template {
            parts: vec![Part::Lit(s.to_string())],
            offset: 0,
            len: 0,
            source_name: String::new(),
        }
    }

    /// Returns the literal text of the template, concatenating literal parts and
    /// rendering `@(name)`/`$(cmd)` references as their textual spelling. Used for
    /// case-pattern matching where the pattern must be a concrete literal.
    pub fn literal_text(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Lit(s) => out.push_str(s),
                Part::Var(name) => out.push_str(name),
                Part::Cmd(_) => {}
            }
        }
        out
    }
}

/// A key-value pair for `env` blocks.
#[derive(Debug, Clone)]
pub struct EnvPair {
    pub key: String,
    pub value: Template,
}

/// A pattern arm inside a `switch` block. Patterns are literal `(...)` text or
/// the `_` default. Only `Default` survives to the runner.
#[derive(Debug, Clone, PartialEq)]
pub enum ArmPattern {
    Lit(String),
    Default,
}
