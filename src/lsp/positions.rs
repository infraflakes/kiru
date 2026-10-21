//! Spans and positions at the protocol boundary.
//!
//! The compiler's spans are byte offsets, and they stay byte offsets inside
//! the compiler. The editor asks for positions in the encoding the two sides
//! negotiated: UTF-8 columns are byte offsets inside the line, and UTF-16
//! columns count code units. The conversion lives here, once, and nothing in
//! the compiler names a protocol type.

use lsp_types::{Position, Range};

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

/// The byte offset a position stands at in the source. A position past the
/// last line clamps to the source's end, and a position inside a line clamps
/// to the line's boundaries.
pub fn position_to_offset(source: &str, position: &Position, encoding: Encoding) -> usize {
    // The starts of every line, in order.
    let mut line_starts = vec![0usize];
    let mut offset = 0usize;
    while let Some(index) = source[offset..].find('\n') {
        offset += index + 1;
        line_starts.push(offset);
    }
    if position.line as usize >= line_starts.len() {
        return source.len();
    }
    let line_start = line_starts[position.line as usize];
    let line_end = source[line_start..]
        .find('\n')
        .map(|index| line_start + index)
        .unwrap_or(source.len());
    let line_text = &source[line_start..line_end];
    match encoding {
        Encoding::Utf8 => line_start + (position.character as usize).min(line_text.len()),
        Encoding::Utf16 => {
            let mut units = 0usize;
            for (byte_index, character) in line_text.char_indices() {
                if units >= position.character as usize {
                    return line_start + byte_index;
                }
                units += char::len_utf16(character);
            }
            line_end
        }
    }
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

    /// Run every round trip, collecting all mismatches so one run reports
    /// every case that failed.
    fn expect_round_trips(cases: &[(&str, Encoding)]) {
        let mut failures = Vec::new();
        for &(text, encoding) in cases {
            for offset in 0..=text.len() {
                if !text.is_char_boundary(offset) {
                    continue;
                }
                let position = offset_to_position(text, offset, encoding);
                let back = position_to_offset(text, &position, encoding);
                if back != offset {
                    failures.push(format!(
                        "{encoding:?} {text:?} at {offset}: got {position:?}, which reads {back}"
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} round trip(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn positions_round_trip_in_both_encodings() {
        expect_round_trips(&[
            ("fn main() { let x: txt = \"a\"; };", Encoding::Utf8),
            (
                "fn main() { let s: txt = \"🦀漢\"; std::print(s); };",
                Encoding::Utf8,
            ),
            (
                "fn main() { let s: txt = \"🦀漢\"; std::print(s); };",
                Encoding::Utf16,
            ),
            ("one\ntwo\nthree\n", Encoding::Utf16),
            ("", Encoding::Utf16),
        ]);
    }

    #[test]
    fn a_position_clamps_to_its_line_and_the_source() {
        let text = "one\ntwo\n";
        // A column past the line's end reads the line's end.
        assert_eq!(
            position_to_offset(
                text,
                &Position {
                    line: 0,
                    character: 9
                },
                Encoding::Utf8
            ),
            3
        );
        // A line past the source's end reads the source's end.
        assert_eq!(
            position_to_offset(
                text,
                &Position {
                    line: 5,
                    character: 0
                },
                Encoding::Utf16
            ),
            8
        );
        // A UTF-16 column lands on the character it counts.
        assert_eq!(
            position_to_offset(
                "🦀x",
                &Position {
                    line: 0,
                    character: 2
                },
                Encoding::Utf16
            ),
            4
        );
        assert_eq!(
            position_to_offset(
                "🦀x",
                &Position {
                    line: 0,
                    character: 3
                },
                Encoding::Utf16
            ),
            5
        );
        // An offset past the source's end clamps to it.
        assert_eq!(
            offset_to_position("two", 99, Encoding::Utf8),
            Position {
                line: 0,
                character: 3
            }
        );
    }
}
