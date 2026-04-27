# Changelog

All notable changes to magna will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to a four-digit `MAJOR.MINOR.PATCH.MICRO` scheme during
the pre-1.0 track. Per-crate stability rules are documented in `GOVERNANCE.md`.

## [Unreleased]

## [0.1.0.0] - 2026-04-27

Initial public release. magna is extracted from the `fw-graph` engine inside
fellwork-api, with full git history preserved via `git filter-repo`. The 72
commits dating back to 2026-03-27 represent the engine's pre-extraction
history; the v0.1 entry below describes what magna ships, not how it got here.

### Added

- **Auto-generated GraphQL from Postgres introspection.** Tables, views,
  functions, enums, and foreign-key relationships are discovered from
  `pg_catalog` and exposed as queries, mutations, connections, and input
  types with no hand-written resolvers.
- **`magna-types`** (stable). Shared types: `StepResult`, `PgValue`,
  `JwtClaims`, error enums. The lingua franca every other crate speaks.
- **`magna-core`** (stable). Two-phase planner / executor. Phase one
  builds an immutable plan from the GraphQL operation; phase two walks it
  against Postgres with batched dataplan steps.
- **`magna-sql`** (stable). Composable SQL AST builder. Produces parameterized
  queries from plan nodes; safe-by-construction (no string concatenation
  on user input).
- **`magna-introspect`** (stable). pg_catalog schema discovery with an
  in-memory cache and `LISTEN`/`NOTIFY`-driven invalidation. DDL changes
  invalidate the cache without restart.
- **`magna-dataplan`** (stable). Postgres-specific data-plan steps:
  `pg_select`, `pg_insert`, `pg_update`, `pg_delete`, `pg_function_call`,
  with batched execution and result merging.
- **`magna-config`** (experimental). Plugin trait, preset registry, YAML
  loader. Tier 1 (`SchemaExtension`) and Tier 2a (field exposure) ship in
  this release; Tier 2b (computed SQL) and Tier 2c (CEL policies) are
  targeted for v0.2.
- **`magna-build`** (experimental). Schema build engine with auto-CRUD
  resolver generation and the `SchemaExtension` trait. `ExtensionContext`
  exposes `register_type`, `query_field`, `mutation_field`, and `add_data`
  for Tier 1 plugins.
- **`magna-serv`** (stable). Axum server with JWT auth, RLS context
  propagation, plan cache, GraphiQL toggle, and structured `tracing`
  spans across the request lifecycle.
- **`magna-subscriptions`** (experimental). Subscriptions backed by
  Postgres `LISTEN`/`NOTIFY`. Channels map to GraphQL subscription fields;
  delivery is best-effort with reconnect.
- **`magna-remote`** (experimental, new in v0.1). Tier 3 HTTP webhook
  resolvers. Engine `POST`s `{args, context, parent}` to the configured
  URL, awaits a JSON response, forwards auth tokens, and emits a tracing
  span around the external call.
- **`magna` binary** (stable CLI, experimental internals). `magna run`,
  `magna export-sdl`, and `magna doctor` for connection diagnostics. Reads
  configuration from `magna.yaml` and environment variables.
- **Extension API.** `SchemaExtension` trait plus `ExtensionContext` with
  `register_type`, `query_field`, `mutation_field`, `add_data`. The same
  Plugin shape is produced internally by Tier 1, Tier 2a, and Tier 3
  inputs. Resolution priority for any field: WASM (v0.5), HTTP webhook,
  computed SQL (v0.2), Rust plugin, auto-generated default.
- **Dual license** MIT or Apache 2.0. License files committed at repo
  root; every `Cargo.toml` declares `license = "MIT OR Apache-2.0"`.
- **Governance.** `GOVERNANCE.md` published with solo-maintainer
  acknowledgment, written succession plan, per-crate stability labels,
  and the license-durability commitment ("no BSL, no SSPL, no
  source-available relicensing; 12 months' notice and a clear fork path
  if that ever needs to change").
- **Tests.** 25 test suites, 367+ tests across the 11 crates. Includes a
  quickstart-level integration test that boots a Postgres container, runs
  `magna`, and asserts a known query response. SDL snapshot test guards
  against unintended schema drift.
- **Documentation.** `README.md` with 5-minute quickstart, `docs/feature-matrix.md`
  with honest supported / partial / unsupported columns vs Postgraphile
  and Hasura, `CHANGELOG.md`, `CONTRIBUTING.md`, `GOVERNANCE.md`.
- **Examples.** `examples/quickstart/` (Docker + Postgres seed schema with
  worked queries).

### Known limitations

- No JSONB filtering operators yet. No array filters, no full-text search,
  no aggregates, no upsert mutations. Targeted for v0.2; tracked in
  `docs/feature-matrix.md`.
- Tier 2b (computed fields via SQL) and Tier 2c (CEL authorization
  policies) are not in v0.1. v0.2 target.
- Tier 4 (WASM component plugins) is not in v0.1. The `magna-wasm` crate
  is not yet present in the workspace; v0.5 target. Crate boundaries are
  designed so adding it is additive, not restructuring.
- Not published to crates.io. Through v0.5, magna is consumed via path
  dependency or pinned git revision. v1.0 switches to crates.io publication.
- Public API of `magna-config`, `magna-build`, and `magna-subscriptions`
  is `experimental` and may break on any minor version through v0.5.
  Breaking changes will always be documented here.

[Unreleased]: https://github.com/fellwork/magna/compare/v0.1.0.0...HEAD
[0.1.0.0]: https://github.com/fellwork/magna/releases/tag/v0.1.0.0
