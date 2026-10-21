//! Byte spans into source text.

/// A half-open byte range `[start, end)` into one source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The smallest span covering both ranges.
    pub(crate) fn merge(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }

    /// Whether the position stands inside the span.
    pub fn contains(self, position: usize) -> bool {
        self.start <= position && position < self.end
    }
}
