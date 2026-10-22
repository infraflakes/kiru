//! Rendering compiler errors with their source location.
//!
//! A diagnostic is one fact: what is wrong or what is unused, and where. It
//! carries a severity and, when one other place explains the fact, the span of
//! that place. The terminal renders an error's own span; the language server
//! publishes the structure as the client asks for it.

use std::path::PathBuf;

use unicode_width::UnicodeWidthChar;

use crate::compiler::LoadedProgram;
use crate::syntax::Span;

/// The name the compiler reports under. The runtime reports under the
/// program's own name instead.
pub(crate) const DIAGNOSTIC_PREFIX: &str = "kc";

/// How a diagnostic weighs. An error stops the compile; a warning states a
/// fact about the source and lets it pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// The kind of unused fact a warning states. The project decides which kinds
/// reach the terminal or the editor; the fact itself is never guessed from
/// its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarningKind {
    UnusedUse,
    UnusedLocal,
    UnusedDeclaration,
}

/// What a diagnostic is: an error, or a warning of one kind. One field, so an
/// error can never carry a warning kind and a warning can never lack one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    Error,
    Warning(WarningKind),
}

impl DiagnosticKind {
    pub fn severity(self) -> Severity {
        match self {
            DiagnosticKind::Error => Severity::Error,
            DiagnosticKind::Warning(_) => Severity::Warning,
        }
    }

    pub fn warning(self) -> Option<WarningKind> {
        match self {
            DiagnosticKind::Error => None,
            DiagnosticKind::Warning(kind) => Some(kind),
        }
    }
}

/// One other place a diagnostic points at: the span and the fact it states.
#[derive(Clone, Debug)]
pub struct RelatedSpan {
    pub path: PathBuf,
    pub span: Span,
    pub message: String,
}

/// The diagnostics of one file, with the source the spans name. A terminal
/// renders from it; the server converts its spans against it.
#[derive(Clone, Debug)]
pub struct FileDiagnostics {
    pub path: PathBuf,
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// Group facts by the file they name, keeping the source the spans convert
/// against and the order they were gathered in. One grouping serves the
/// terminal's failure and the server's publication.
pub(crate) fn group(
    loaded: &LoadedProgram,
    diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> Vec<FileDiagnostics> {
    let mut files: Vec<FileDiagnostics> = Vec::new();
    for diagnostic in diagnostics {
        if let Some(file) = files.iter_mut().find(|file| file.path == diagnostic.path) {
            file.diagnostics.push(diagnostic);
            continue;
        }
        let source = loaded
            .files
            .iter()
            .find(|file| file.path == diagnostic.path)
            .map(|file| file.source.clone())
            .unwrap_or_default();
        files.push(FileDiagnostics {
            path: diagnostic.path.clone(),
            source,
            diagnostics: vec![diagnostic],
        });
    }
    files
}

/// One compiler fact tied to a source file and byte range.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub span: Span,
    pub message: String,
    pub kind: DiagnosticKind,
    /// The places that explain this fact, when one exists. The first
    /// declaration behind a duplicate carries its span here.
    pub related: Vec<RelatedSpan>,
}

impl Diagnostic {
    pub(crate) fn new(path: impl Into<PathBuf>, span: Span, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            span,
            message: message.into(),
            kind: DiagnosticKind::Error,
            related: Vec::new(),
        }
    }

    /// The same fact, weighed as a warning of one kind.
    pub(crate) fn warning(
        path: impl Into<PathBuf>,
        span: Span,
        message: impl Into<String>,
        kind: WarningKind,
    ) -> Self {
        Self {
            kind: DiagnosticKind::Warning(kind),
            ..Self::new(path, span, message)
        }
    }

    /// Point at one other place that explains the fact.
    pub(crate) fn with_related(
        mut self,
        path: impl Into<PathBuf>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        self.related.push(RelatedSpan {
            path: path.into(),
            span,
            message: message.into(),
        });
        self
    }

    /// The severity of the fact.
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }

    /// The unused fact this warning states, when it is one. A project's
    /// diagnostics policy reads this, never the message.
    pub fn warning_kind(&self) -> Option<WarningKind> {
        self.kind.warning()
    }

    /// The word the terminal renders for the severity.
    fn severity_word(&self) -> &'static str {
        match self.severity() {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }

    /// Render as `path:line:column: error: message`, followed by every line
    /// the span covers and a caret underline. Tabs are expanded to four-column
    /// stops and characters take their Unicode display width, so the underline
    /// stays aligned. Related spans are structure, not rendering: a terminal
    /// reads the one fact at its own place, and a client that wants the related
    /// places reads the diagnostic's fields.
    pub(crate) fn render(&self, source: &str) -> String {
        let start = self.span.start.min(source.len());
        let end = self.span.end.min(source.len()).max(start);
        let line_start = source[..start]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let line_number = source[..start].matches('\n').count() + 1;
        let column = display_width(&source[line_start..start]) + 1;

        let mut rendered = format!(
            "{}:{line_number}:{}: {}: {}\n",
            self.path.display(),
            column,
            self.severity_word(),
            self.message
        );

        let mut position = line_start;
        loop {
            let line_end = source[position..]
                .find('\n')
                .map(|index| position + index)
                .unwrap_or(source.len());
            rendered.push_str(&expand_tabs(&source[position..line_end]));
            rendered.push('\n');

            let underline_from = start.max(position);
            let underline_to = end.min(line_end);
            if underline_from <= underline_to {
                let indent = display_width(&source[position..underline_from]);
                let width = display_width(&source[underline_from..underline_to]).max(1);
                rendered.push_str(&" ".repeat(indent));
                rendered.push_str(&"^".repeat(width));
                rendered.push('\n');
            }

            if end <= line_end || line_end >= source.len() {
                break;
            }
            position = line_end + 1;
        }
        rendered
    }
}

/// The width of one tab stop in columns.
const TAB_WIDTH: usize = 4;

/// The column that follows `character` when it is appended at `column`. A tab
/// advances to the next tab stop, a combining mark occupies no column, and
/// every other displayable character occupies its Unicode width.
fn next_column(column: usize, character: char) -> usize {
    if character == '\t' {
        column + TAB_WIDTH - (column % TAB_WIDTH)
    } else {
        column + character.width().unwrap_or(0)
    }
}

/// The number of terminal columns a text occupies, counting a tab to the next
/// four-column stop and a combining mark to no column.
fn display_width(text: &str) -> usize {
    text.chars().fold(0, next_column)
}

fn expand_tabs(text: &str) -> String {
    let mut expanded = String::new();
    let mut width = 0;
    for character in text.chars() {
        let next = next_column(width, character);
        if character == '\t' {
            expanded.push_str(&" ".repeat(next - width));
        } else {
            expanded.push(character);
        }
        width = next;
    }
    expanded
}

/// Render a diagnostic against the source of the file it points at.
pub(in crate::compiler) fn render(program: &LoadedProgram, diagnostic: &Diagnostic) -> String {
    let source = program
        .files
        .iter()
        .find(|file| file.path == diagnostic.path)
        .map(|file| file.source.as_str())
        .unwrap_or("");
    diagnostic.render(source)
}

/// A count and its noun, pluralized: `1 argument`, `2 arguments`. Every arity
/// diagnostic reads through this, so the singular is never wrong.
pub(crate) fn count(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// The message for a variant payload that does not carry one item per declared
/// payload type, or `None` when it does. `subject` is how the site writes the
/// variant and `noun` names its items there, so a construction and a pattern
/// share the count check while each keeps its own noun.
pub(crate) fn variant_payload_count(
    subject: &str,
    declared: usize,
    found: usize,
    noun: &str,
) -> Option<String> {
    (found != declared)
        .then(|| format!("`{subject}` takes {}, found {found}", count(declared, noun)))
}

/// The message for a value of the wrong type: `expected <required>, found
/// <actual>`, where both names come from the type system.
pub(crate) fn expected_instead(required: &str, actual: &str) -> String {
    format!("expected {required}, found {actual}")
}

/// The message for a native named where a value is required. A native has no
/// runtime value yet: a function value is a code pointer, and a native is
/// dispatched by index.
pub(crate) fn native_used_as_value(name: &str) -> String {
    format!("`{name}` is a native and has no value form; call it")
}

/// The message for a value called as a function.
pub(crate) fn value_called(name: &str) -> String {
    format!("`{name}` is a value and cannot be called")
}

/// The message for a name declared more than once in one scope.
pub(crate) fn duplicate_name(name: &str) -> String {
    format!("`{name}` is declared more than once")
}

/// The message for a record literal that repeats a field name.
pub(crate) fn duplicate_field(name: &str) -> String {
    format!("duplicate field `{name}`")
}

/// The message for a name declared more than once in one namespace.
pub(crate) fn duplicate_in_namespace(name: &str) -> String {
    format!("`{name}` is declared more than once in this namespace")
}

/// The message a duplicate points back with: the fact that the place it
/// names is the first declaration of the name.
pub(crate) fn first_declaration(name: &str) -> String {
    format!("first declaration of `{name}`")
}

/// The message for a field or variant reached across a namespace that does not
/// mark it `pub`. A member is private to its own namespace, exactly like a
/// declaration.
pub(crate) fn private_member(owner: &str, member: &str, namespace: &str) -> String {
    format!("`{owner}.{member}` is private to `{namespace}`; mark it `pub` to reach it")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run every rendering case, collecting all mismatches so one run reports
    /// every case that failed.
    fn expect_renders(cases: &[(&str, Span, &str, &str, &str)]) {
        let mut failures = Vec::new();
        for &(name, span, message, source, expected) in cases {
            let rendered = Diagnostic::new("main.kiru", span, message).render(source);
            if rendered != expected {
                failures.push(format!("{name}: expected:\n{expected}\nfound:\n{rendered}"));
            }
        }
        assert!(
            failures.is_empty(),
            "{} rendering case(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn renders_spans() {
        expect_renders(&[
            (
                "single-line span",
                Span::new(13, 17),
                "unknown name",
                "fn main() { nope(); };\n",
                "main.kiru:1:14: error: unknown name\nfn main() { nope(); };\n             ^^^^\n",
            ),
            (
                "multiline span",
                Span::new(13, 22),
                "bad string",
                "let s: txt = \"one\ntwo\";\n",
                "main.kiru:1:14: error: bad string\nlet s: txt = \"one\n             ^^^^\ntwo\";\n^^^^\n",
            ),
            (
                "carets aligned after tabs",
                Span::new(1, 6),
                "bad value",
                "\tvalue\n",
                "main.kiru:1:5: error: bad value\n    value\n    ^^^^^\n",
            ),
        ]);
    }

    #[test]
    fn caret_aligns_after_a_wide_character() {
        expect_renders(&[(
            "wide character before the span",
            Span::new(3, 4),
            "unknown name",
            "漢x\n",
            "main.kiru:1:3: error: unknown name\n漢x\n  ^\n",
        )]);
    }
}
