//! Byte spans over the source text and line/column resolution.

use serde::Serialize;

/// A half-open byte range `[start, end)` into the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// A zero-width span, used to point at a position where something is missing.
    pub const fn empty(at: usize) -> Self {
        Self::new(at, at)
    }

    pub const fn len(self) -> usize {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The smallest span covering both `self` and `other`.
    pub fn to(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }

    pub fn slice(self, src: &str) -> &str {
        &src[self.start..self.end]
    }
}

/// Resolves byte offsets to 1-based lines and columns.
pub struct SourceMap<'a> {
    src: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> SourceMap<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        Self { src, line_starts }
    }

    /// The 1-based line containing `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&start| start <= offset)
    }

    /// The byte offset where a 1-based line starts.
    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts[line - 1]
    }

    /// The 1-based line and column of `offset`. Columns count characters, not bytes.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let line = self.line_of(offset);
        let start = self.line_start(line);
        let col = self.src[start..offset.min(self.src.len())].chars().count() + 1;
        (line, col)
    }

    /// The text of a 1-based line, without its terminator.
    pub fn line_text(&self, line: usize) -> &'a str {
        let start = self.line_start(line);
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.src.len());
        self.src[start..end].trim_end_matches(['\n', '\r'])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_lines_and_columns() {
        let map = SourceMap::new("feat: a\n\nbody é x");
        assert_eq!(map.line_col(0), (1, 1));
        assert_eq!(map.line_col(7), (1, 8));
        assert_eq!(map.line_col(8), (2, 1));
        assert_eq!(map.line_col(16), (3, 7));
        assert_eq!(map.line_text(1), "feat: a");
        assert_eq!(map.line_text(2), "");
        assert_eq!(map.line_text(3), "body é x");
    }

    #[test]
    fn end_of_input_lands_on_the_last_line() {
        let map = SourceMap::new("feat: a\n");
        assert_eq!(map.line_col(8), (2, 1));
        assert_eq!(map.line_text(2), "");
    }
}
