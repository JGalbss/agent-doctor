//! Source-position utilities shared by every analysis that turns a byte offset
//! (oxc spans are byte offsets) into a 1-based line/column plus the source line
//! for code-frame snippets. Centralised so the line-start table is computed and
//! searched one way everywhere.

/// Precomputed byte offsets of each line start, for O(log n) offset → line/col.
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    /// Build the line-start table for a source string.
    pub fn new(source: &str) -> LineIndex {
        let starts = std::iter::once(0)
            .chain(
                source
                    .bytes()
                    .enumerate()
                    .filter(|(_, byte)| *byte == b'\n')
                    .map(|(offset, _)| offset + 1),
            )
            .collect();
        LineIndex { starts }
    }

    /// Zero-based index of the line containing `offset`.
    fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index - 1,
        }
    }

    /// 1-based `(line, column)` of a byte offset.
    pub fn line_col(&self, offset: usize) -> (u32, u32) {
        let line_index = self.line_of(offset);
        let line_start = self.starts[line_index];
        ((line_index + 1) as u32, (offset - line_start + 1) as u32)
    }

    /// The source line containing `offset`, with trailing whitespace trimmed —
    /// the snippet shown in a diagnostic's code frame.
    pub fn line_text<'s>(&self, source: &'s str, offset: usize) -> &'s str {
        let line_start = self.starts[self.line_of(offset)];
        let line_end = source[line_start..]
            .find('\n')
            .map(|relative| line_start + relative)
            .unwrap_or(source.len());
        source[line_start..line_end].trim_end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_is_one_based() {
        let source = "a\nbc\nd";
        let index = LineIndex::new(source);
        assert_eq!(index.line_col(0), (1, 1)); // 'a'
        assert_eq!(index.line_col(2), (2, 1)); // 'b'
        assert_eq!(index.line_col(3), (2, 2)); // 'c'
        assert_eq!(index.line_col(5), (3, 1)); // 'd'
    }

    #[test]
    fn line_text_trims_and_bounds() {
        let source = "first  \nsecond line\n";
        let index = LineIndex::new(source);
        assert_eq!(index.line_text(source, 0), "first");
        assert_eq!(index.line_text(source, 8), "second line");
    }
}
