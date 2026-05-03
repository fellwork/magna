// SPDX-License-Identifier: MIT OR Apache-2.0
//! Smoke test for the `serde` feature scaffold (R4).
//!
//! Real `#[derive(Serialize, Deserialize)]` on AST types is DEFERRED until
//! after R3's bumpalo migration lands — we don't want to fight a moving AST
//! shape this round. For now this file just confirms:
//!
//! 1. The crate compiles with `--features serde`.
//! 2. Public types are reachable from a downstream crate (the test binary).
//!
//! A post-R3 round will add real serde-roundtrip assertions.

#![cfg(feature = "serde")]

#[test]
fn serde_feature_compiles_and_imports_resolve() {
    // Just construct a Span via the public API. This is a no-op at runtime
    // but ensures we haven't accidentally broken the public re-exports under
    // the `serde` feature.
    let _ = magna_gqlmin::Span { start: 0, end: 0 };
    let _ = magna_gqlmin::ParseErrorKind::UnexpectedEof;
}
