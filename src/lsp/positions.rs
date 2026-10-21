//! Spans and positions at the protocol boundary.
//!
//! The compiler's spans are byte offsets, and they stay byte offsets inside
//! the compiler. The editor asks for positions in the encoding the two sides
//! negotiated: UTF-8 columns are byte offsets inside the line, and UTF-16
//! columns count code units. The conversion lives here, once, and nothing in
//! the compiler names a protocol type.

use tower_lsp_server::ls_types::{Position, Range};

use crate::syntax::Span;

/// The encoding a position's columns count in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

/// The position a byte offset stands at in the source.
pub fn offset_to_position(source: &str, offset: usize, encoding: Encoding) -> Position {
    let offset = offset.min(source.len());
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let line = source[..line_start].matches('\n').count() as u32;
    let within = &source[line_start..offset];
    let character = match encoding {
        Encoding::Utf8 => within.len() as u32,
        Encoding::Utf16 => within.chars().map(char::len_utf16).sum::<usize>() as u32,
    };
    Position { line, character }
}

/// The range a span covers in the source.
pub fn span_to_range(source: &str, span: Span, encoding: Encoding) -> Range {
    Range {
        start: offset_to_position(source, span.start, encoding),
        end: offset_to_position(source, span.end, encoding),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_maps_to_its_lines_and_columns() {
        let source = "one\ntwo 🦀\nthree";
        let span = Span::new(4, 7);
        assert_eq!(
            span_to_range(source, span, Encoding::Utf8),
            lsp(1, 0, 1, 3),
            "a byte range maps to its line and byte columns"
        );
        assert_eq!(
            span_to_range(source, span, Encoding::Utf16),
            lsp(1, 0, 1, 3),
            "the same range in code units"
        );
        let crab = Span::new(8, 12);
        assert_eq!(
            span_to_range(source, crab, Encoding::Utf8),
            lsp(1, 4, 1, 8),
            "a non-BMP character spans its four bytes"
        );
        assert_eq!(
            span_to_range(source, crab, Encoding::Utf16),
            lsp(1, 4, 1, 6),
            "the same character counts two code units"
        );
    }

    /// A range written as (start line, start column, end line, end column).
    fn lsp(
        start_line: u32,
        start_character: u32,
        end_line: u32,
        end_character: u32,
    ) -> tower_lsp_server::ls_types::Range {
        tower_lsp_server::ls_types::Range {
            start: tower_lsp_server::ls_types::Position {
                line: start_line,
                character: start_character,
            },
            end: tower_lsp_server::ls_types::Position {
                line: end_line,
                character: end_character,
            },
        }
    }
}
