//! Parsing layer: salsa inputs + line index. The prism AST is lifetime-bound,
//! so queries re-parse locally and return owned data (prism is fast; salsa's
//! structural early-cutoff on the owned results preserves incrementality).

use std::path::PathBuf;

pub use ruby_prism;

/// A source file tracked by salsa. `text` is the full current contents.
#[salsa::input(debug)]
pub struct SourceFile {
    #[returns(ref)]
    pub path: PathBuf,
    #[returns(ref)]
    pub text: String,
}

/// Byte-offset -> (line, col) conversion, both 0-based. Cols are UTF-16 code
/// units within the line, per the LSP default encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineIndex {
    line_starts: Vec<u32>,
}

/// `u32` is not a size choice here, it is the protocol's type: an LSP
/// `Position` carries `u32` line and character fields, so a source file
/// whose offsets exceed `u32::MAX` (4 GiB) cannot be addressed over the
/// wire at all — prism will not parse one either. The casts below are
/// bounded by that ceiling, and `expect` rather than `allow` means this
/// suppression fails the build the day the ceiling stops being real.
#[expect(
    clippy::cast_possible_truncation,
    reason = "offsets are bounded by the LSP wire format's own u32 Position; see the note above"
)]
impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        Self { line_starts }
    }

    /// 0-based (line, utf16-col) for a byte offset. Offsets past EOF clamp.
    pub fn line_col(&self, text: &str, offset: usize) -> (u32, u32) {
        let offset = offset.min(text.len());
        let line = match self.line_starts.binary_search(&(offset as u32)) {
            Ok(l) => l,
            Err(l) => l - 1,
        };
        let line_start = self.line_starts[line] as usize;
        let col = text[line_start..offset]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>() as u32;
        (line as u32, col)
    }

    /// Byte offset of the start of a 0-based line (clamped to last line).
    pub fn line_start(&self, line: u32) -> usize {
        let line = (line as usize).min(self.line_starts.len() - 1);
        self.line_starts[line] as usize
    }

    /// Byte offset for a 0-based (line, utf16-col) position, clamped.
    pub fn offset(&self, text: &str, line: u32, col: u32) -> usize {
        let start = self.line_start(line);
        let end = self
            .line_starts
            .get(line as usize + 1)
            .map_or(text.len(), |s| *s as usize);
        let mut units = 0u32;
        for (i, c) in text[start..end].char_indices() {
            if units >= col {
                return start + i;
            }
            units += c.len_utf16() as u32;
        }
        end
    }
}
