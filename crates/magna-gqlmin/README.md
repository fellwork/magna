# magna-gqlmin

**Stability: experimental.** Breaking changes may land in any minor version on
the 0.x track without a separate signal beyond the `CHANGELOG.md` entry. See
[`GOVERNANCE.md`](../../GOVERNANCE.md) for the full stability policy.

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
