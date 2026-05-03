// SPDX-License-Identifier: MIT OR Apache-2.0
//! Caret-style error rendering for [`ParseError`].
//!
//! Gated behind the `pretty` feature, which implies `std`. This module is
//! intentionally NOT pulled into the wasm runtime build — formatting tables
//! and `String` allocation would blow the size budget.
//!
//! Output format (mirrors rustc and miette's basic shape):
//!
//! ```text
//! error: <kind message>
//!   --> <line>:<column>
//!    |
//! N  | <source line>
//!    | <spaces>^^^^
//! ```
//!
//! The caret count equals `span.len()` (clamped to at least 1) so multi-byte
//! errors highlight their full extent. Column is reported as a 1-indexed
//! BYTE offset within the line, not a grapheme or display-width column —
//! this keeps the implementation table-free and predictable for ASCII
//! GraphQL sources, which is the dominant case. Multi-byte UTF-8 sources
//! will render correctly in monospaced fonts that treat each byte position
//! as a column.

use crate::ParseError;

impl ParseError {
    /// Render a multi-line diagnostic with a caret pointer for the offending span.
    ///
    /// The returned `String` ends with a trailing newline so it can be
    /// `println!`-ed or written directly to stderr.
    ///
    /// # Notes
    ///
    /// * Column is a 1-indexed byte offset within the line. For ASCII
    ///   GraphQL (the overwhelming majority), this matches the visual
    ///   column. See module docs for the rationale.
    /// * If `span.start` lies past the last newline (e.g. an EOF error),
    ///   the rendered line is the final line of input (which may be empty).
    pub fn render(&self, src: &str) -> String {
        // Resolve line / column / line text from the byte offset.
        let start = self.span.start as usize;
        let end = self.span.end as usize;
        // Clamp into [0, src.len()] so EOF spans don't panic.
        let start = start.min(src.len());
        let end = end.min(src.len()).max(start);

        // Find the start of the line containing `start`. Walk back from
        // `start` to the previous '\n' (exclusive), or to 0.
        let line_start = src[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        // Find the end of the line: the next '\n' from `line_start`, or end-of-src.
        let line_end = src[line_start..]
            .find('\n')
            .map(|i| line_start + i)
            .unwrap_or(src.len());
        let line_text = &src[line_start..line_end];

        // 1-indexed line number == count of '\n' in src[..line_start] + 1.
        let line_no = src[..line_start].bytes().filter(|&b| b == b'\n').count() + 1;
        // 1-indexed byte column within the line.
        let column = start - line_start + 1;

        // Caret count: at least 1, even for zero-length (EOF) spans.
        let caret_len = (end - start).max(1);

        // Width of the line-number gutter (e.g. "12  | ").
        let line_no_str = line_no.to_string();
        let gutter_width = line_no_str.len();

        let mut out = String::with_capacity(64 + line_text.len() + caret_len);
        out.push_str("error: ");
        out.push_str(self.kind.message());
        out.push('\n');

        // "  --> <line>:<column>"
        push_repeat(&mut out, ' ', gutter_width);
        out.push_str("  --> ");
        out.push_str(&line_no_str);
        out.push(':');
        out.push_str(&column.to_string());
        out.push('\n');

        // " <gutter> |"
        push_repeat(&mut out, ' ', gutter_width);
        out.push_str("  |\n");

        // "<N>  | <source line>"
        out.push_str(&line_no_str);
        out.push_str("  | ");
        out.push_str(line_text);
        out.push('\n');

        // " <gutter>  | <spaces>^^^^"
        push_repeat(&mut out, ' ', gutter_width);
        out.push_str("  | ");
        // Pad with spaces up to the column. Use one space per byte in the
        // line prefix so monospaced fonts align the caret.
        push_repeat(&mut out, ' ', column - 1);
        push_repeat(&mut out, '^', caret_len);
        out.push('\n');

        out
    }
}

fn push_repeat(s: &mut String, c: char, n: usize) {
    for _ in 0..n {
        s.push(c);
    }
}
