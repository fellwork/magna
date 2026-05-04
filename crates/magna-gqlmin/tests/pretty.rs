// SPDX-License-Identifier: MIT OR Apache-2.0
//! Caret-renderer tests (R4, step 10 partial).
//!
//! Each test feeds a hand-crafted source and `ParseError`, then asserts on
//! substrings of the rendered output rather than the full string — this
//! avoids brittle whitespace coupling while still verifying every load-bearing
//! component (line number, column, caret count, gutter, source line).

#![cfg(feature = "pretty")]

use magna_gqlmin::{ParseError, ParseErrorKind, Span};

fn err(start: u32, end: u32, kind: ParseErrorKind) -> ParseError {
    ParseError::new(Span::new(start, end), kind)
}

#[test]
fn renders_single_line_source_at_line_one() {
    let src = "query { foo }";
    // Span over the `foo` field, bytes 8..11
    let e = err(8, 11, ParseErrorKind::ExpectedName);
    let out = e.render(src);

    assert!(out.contains("error: "), "missing error prefix: {out}");
    assert!(out.contains(ParseErrorKind::ExpectedName.message()));
    assert!(out.contains("--> 1:9"), "expected '--> 1:9' in {out}");
    assert!(out.contains("query { foo }"), "missing source line: {out}");
    // Caret count: span len = 3
    assert!(out.contains("^^^"), "expected 3 carets: {out}");
    assert!(!out.contains("^^^^"), "too many carets: {out}");
}

#[test]
fn renders_multi_line_source_locates_correct_line() {
    let src = "query Foo {\n  user {\n    naem\n  }\n}\n";
    // Locate "naem" — bytes from `naem` start to end.
    let start = src.find("naem").unwrap() as u32;
    let e = err(start, start + 4, ParseErrorKind::ExpectedName);
    let out = e.render(src);

    assert!(out.contains("--> 3:5"), "wrong line/col: {out}");
    // Source line preserved verbatim (no trailing newline).
    assert!(out.contains("    naem"), "missing source line: {out}");
    // Caret count: 4
    assert!(out.contains("^^^^"), "missing 4 carets: {out}");
    // Other lines NOT included (we only show the failing line — no context).
    assert!(!out.contains("user {"), "context line leaked into output: {out}");
}

#[test]
fn renders_eof_span_at_end_of_input() {
    let src = "query { foo";
    let len = src.len() as u32;
    // Zero-length span at EOF.
    let e = err(len, len, ParseErrorKind::UnexpectedEof);
    let out = e.render(src);

    // Single-line source so line 1; column = byte offset + 1.
    assert!(out.contains("--> 1:"), "missing --> prefix: {out}");
    // Caret is at least 1 even for empty span.
    assert!(out.contains('^'), "missing caret: {out}");
    // Source line shown.
    assert!(out.contains("query { foo"), "missing source line: {out}");
}

#[test]
fn caret_count_matches_span_length() {
    let src = "fragment X on T { f }";
    // Span over "fragment" — 8 bytes.
    let e = err(0, 8, ParseErrorKind::UnexpectedToken);
    let out = e.render(src);

    // Exactly 8 carets.
    let carets: usize = out.chars().filter(|&c| c == '^').count();
    assert_eq!(carets, 8, "expected 8 carets, got {carets}: {out}");
}

#[test]
fn renders_error_on_first_line_with_lf_terminator() {
    let src = "query{\n}\n";
    // The empty selection set body — `EmptySelectionSet` would point at `}`.
    let e = err(7, 8, ParseErrorKind::EmptySelectionSet);
    let out = e.render(src);

    assert!(out.contains("--> 2:1"), "wrong line/col: {out}");
    assert!(out.contains("^"), "missing caret: {out}");
}
