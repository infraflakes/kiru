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

    /// Whether a position stands inside the span.
    pub(crate) fn contains(self, position: usize) -> bool {
        self.start <= position && position < self.end
    }

    /// Whether another span sits inside this one.
    pub(crate) fn covers(self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}
