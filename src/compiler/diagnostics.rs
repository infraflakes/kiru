//! Rendering compiler errors with their source location.

use std::path::PathBuf;

use crate::syntax::Span;

/// One compiler error tied to a source file and byte range.
#[derive(Debug)]
pub(crate) struct Diagnostic {
    pub(crate) path: PathBuf,
    pub(crate) span: Span,
    pub(crate) message: String,
}

impl Diagnostic {
    pub(crate) fn new(path: impl Into<PathBuf>, span: Span, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            span,
            message: message.into(),
        }
    }

    /// Render as `path:line:column: error: message`, followed by every line
    /// the span covers and a caret underline. Tabs are expanded to four-column
    /// stops so the underline stays aligned.
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
            "{}:{line_number}:{column}: error: {}\n",
            self.path.display(),
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
/// advances to the next tab stop, every other character occupies one column.
fn next_column(column: usize, character: char) -> usize {
    if character == '\t' {
        column + TAB_WIDTH - (column % TAB_WIDTH)
    } else {
        column + 1
    }
}

/// The number of terminal columns a text occupies, counting a tab to the next
/// four-column stop.
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
                Span::new(8, 17),
                "bad string",
                "var s = \"one\ntwo\";\n",
                "main.kiru:1:9: error: bad string\nvar s = \"one\n        ^^^^\ntwo\";\n^^^^\n",
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
}
