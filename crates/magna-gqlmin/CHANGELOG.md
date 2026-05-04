# magna-gqlmin Changelog

This crate uses Semantic Versioning. As an `experimental` crate per
GOVERNANCE.md, breaking API changes are allowed in 0.x minor versions.

## [0.1.0] — Unreleased

Initial release. Operations-only GraphQL parser with three distribution
modes (native Rust, wasm32 via build-std nightly, napi-rs).

### Added

- DFA lexer with full Oct-2021 spec coverage
- LL(1) recursive-descent operations parser with span-indexed Node arena
- 20-case acceptance corpus
- 5 ops-only validation rules: NoUndefinedVariables, NoUnusedVariables,
  NoUnusedFragments, KnownFragmentNames, UniqueOperationNames
- Pretty error rendering with caret diagnostics (`pretty` feature)
- AST serde derives (`serde` feature)
- napi-rs binding (`napi` feature)
- Custom 256 KiB inline bump allocator for wasm builds (zero runtime deps)

### Size

- 6.0 KB gz wasm operations parser (nightly build-std + immediate-abort + custom allocator)

### Stability

- `experimental` per GOVERNANCE.md. AST shape and public API may change in 0.x minors.
