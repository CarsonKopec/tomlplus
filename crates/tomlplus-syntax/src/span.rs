//! Source spans — half-open byte ranges into the source text.

use std::ops::Range;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const DUMMY: Span = Span { start: 0, end: 0 };

    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    pub fn point(offset: usize) -> Self {
        Self { start: offset, end: offset }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    pub fn len(self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

impl From<Span> for Range<usize> {
    fn from(s: Span) -> Self {
        s.start..s.end
    }
}

/// Maps byte offsets to (line, column) using UTF-8 byte counts.
/// Lines are 0-indexed; columns are byte offsets within their line.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset where each line starts. Always begins with 0.
    line_starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts, len: text.len() }
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Returns (line, column) for a byte offset. Both 0-indexed.
    pub fn position(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.len);
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let col = offset - self.line_starts[line];
        (line as u32, col as u32)
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts.get(line).copied().unwrap_or(self.len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_basic() {
        let idx = LineIndex::new("abc\ndef\n\nghi");
        assert_eq!(idx.position(0), (0, 0));
        assert_eq!(idx.position(3), (0, 3));
        assert_eq!(idx.position(4), (1, 0));
        assert_eq!(idx.position(8), (2, 0));
        assert_eq!(idx.position(9), (3, 0));
    }

    #[test]
    fn span_merge() {
        let a = Span::new(2, 5);
        let b = Span::new(8, 10);
        assert_eq!(a.merge(b), Span::new(2, 10));
    }
}
