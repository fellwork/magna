# magna-gqlmin

**Stability: experimental.** Breaking changes may land in any minor version on
the 0.x track without a separate signal beyond the `CHANGELOG.md` entry. See
[`GOVERNANCE.md`](../../GOVERNANCE.md) for the full stability policy.

## Size

The wasm operations parser ships at **6.0 KB gz** (gzipped, post
`wasm-opt -Oz`) on nightly Rust with `-Z build-std=core,alloc` +
`-Cpanic=immediate-abort`. The wasm distribution has zero runtime
crate dependencies — a custom inline bump allocator replaces dlmalloc.

The native (default-features) build links the same parser without
the wasm-only allocator and is suitable for build-time tooling and
the eventual SFC compiler integration.

See `SIZE.md` for the full reduction journey (R2 baseline 15,375 →
final 6,155 = −60%).

A lightweight GraphQL parser sized for three distribution modes from a single
Rust source: a `wasm32-unknown-unknown` runtime build with a hard ≤5 KB gz
budget, a napi-rs binding for Node/Bun consumers, and a native Rust dependency
for build-time tooling that includes optional SDL parsing and validation. The
crate is hand-written (DFA lexer + LL(1) recursive descent), no_std-capable,
and has zero runtime dependencies for the default `ops + std` build. Round 1
delivers the operations parser and a 20-case corpus; SDL, validation, pretty
errors, napi, and the wasm pipeline are gated behind opt-in features and
landed in later rounds.

## License

Dual-licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.
