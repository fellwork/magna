# magna feature matrix

This is magna's accountability ledger: an honest audit of what works at v0.1 versus Postgraphile (Node.js) and Hasura (Haskell). Every "supported" claim links to the implementing module or example so you can verify it; every "partial" claim names what's missing; every "not supported" claim points at a tracking issue or the v0.x release that targets it. We update this matrix in the same PR that changes feature support.

**Legend:**

- `[YES]` — supported at v0.1, with a file pointer to the implementing code or example.
- `[PARTIAL]` — works for the common case but is missing capability called out inline; links to the v0.x target.
- `[NO]` — not supported; links to the tracking issue or names the target version.
- `[OUT OF SCOPE]` — deliberately not pursued; see the "Deliberately not pursued" section for rationale.

Postgraphile and Hasura columns reflect their stable releases as of 2026-04. Where their docs were ambiguous we erred on the side of crediting them.

---

## Schema generation

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| Auto-generate object types from tables | `[YES]` (`crates/magna-build/src/register/object_types.rs`) | `[YES]` | `[YES]` |
| Auto-generate types from views | `[YES]` (`crates/magna-introspect/src/lib.rs`) | `[YES]` | `[YES]` |
| Auto-generate types from materialized views | `[PARTIAL]` reads only; refresh mutation tracked for v0.2 | `[YES]` | `[YES]` |
| Auto-generate fields from Postgres functions | `[PARTIAL]` scalar-returning functions only (`crates/magna-build/src/register/functions.rs`); set-returning + composite returns target v0.2 | `[YES]` | `[YES]` |
| Computed columns from SQL expressions (no pg function) | `[NO]` Tier 2b targeted v0.2 | `[YES]` (smart tag `@computed`) | `[YES]` (computed fields metadata) |
| Custom types via Rust trait (`SchemaExtension`) | `[YES]` (`crates/magna-build/src/extension.rs`) | `[PARTIAL]` JS plugin system, undocumented contract | `[NO]` (closest equivalent is remote schemas / actions) |
| Custom types via YAML config | `[PARTIAL]` Tier 2a (field exposure, rename, omit) only at v0.1; computed-via-SQL + policies target v0.2 | `[YES]` (smart tags) | `[YES]` (metadata API) |
| Custom types via WASM component plugins | `[NO]` Tier 4 targeted v0.5 (`magna-wasm` crate planned) | `[NO]` | `[NO]` |
| Naming overrides (rename table / column in API) | `[YES]` (Tier 2a config: `rename_as`) | `[YES]` | `[YES]` |
| Hide columns / tables from generated schema | `[YES]` (`omit_fields`, `expose: false`) | `[YES]` | `[YES]` |
| Auto camelCase fields | `[YES]` plugin toggle (`auto_camelcase: true`) | `[YES]` (default) | `[YES]` (configurable) |
| Relay-style connections | `[YES]` plugin toggle (`relay_connections`); compiles to keyset pagination | `[YES]` (default) | `[NO]` (offset/limit only) |
| Composite primary keys end-to-end | `[PARTIAL]` reads work; mutations + relations targeted v0.2 | `[YES]` | `[YES]` |
| Multi-schema discovery | `[YES]` (`magna-introspect` accepts a schema list) | `[YES]` | `[YES]` |
| SDL export from CLI | `[NO]` `magna export-sdl` targeted v0.2 | `[YES]` | `[YES]` |

## Query operators

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| Equality (`=`, `!=`) | `[YES]` | `[YES]` | `[YES]` |
| Comparison (`<`, `<=`, `>`, `>=`) | `[YES]` | `[YES]` | `[YES]` |
| `IN` / `NOT IN` | `[YES]` | `[YES]` | `[YES]` |
| `IS NULL` / `IS NOT NULL` | `[YES]` | `[YES]` | `[YES]` |
| `BETWEEN` | `[PARTIAL]` expressible via `gte` + `lte`; native `between` operator targeted v0.2 | `[YES]` | `[YES]` |
| String `LIKE` / `ILIKE` | `[YES]` | `[YES]` | `[YES]` |
| Regex (`~`, `~*`) | `[PARTIAL]` exposed only on `text` columns; broader coverage targeted v0.2 | `[YES]` | `[YES]` |
| JSONB `?` (has key) | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| JSONB `@>` (contains) | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| JSONB path (`#>`, `jsonb_path_exists`) | `[NO]` targeted v0.2 | `[PARTIAL]` (path queries via smart tags) | `[YES]` |
| Array `@>` / `&&` (contains / overlap) | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| Array element-of filter | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| Full-text search (`tsvector @@ tsquery`) | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| Geographic (PostGIS) operators | `[OUT OF SCOPE]` for v0.x; recommended path is a Tier 1 plugin or Tier 3 webhook | `[PARTIAL]` (extension) | `[PARTIAL]` (extension) |
| Boolean combinators (`_and`, `_or`, `_not`) | `[YES]` | `[YES]` | `[YES]` |

## Filtering, sorting, pagination

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| Filter by column scalar value | `[YES]` | `[YES]` | `[YES]` |
| Filter on relations (nested where) | `[YES]` | `[YES]` | `[YES]` |
| Order by column ascending / descending | `[YES]` | `[YES]` | `[YES]` |
| Order by multiple columns | `[YES]` | `[YES]` | `[YES]` |
| Order by relation field | `[PARTIAL]` single-hop only; multi-hop targeted v0.2 | `[YES]` | `[YES]` |
| Order by computed expression | `[NO]` lands with Tier 2b in v0.2 | `[YES]` | `[PARTIAL]` |
| Offset + limit pagination | `[YES]` | `[YES]` | `[YES]` |
| Keyset / cursor pagination | `[YES]` (default for Relay connections) | `[YES]` | `[NO]` |
| Total count alongside page | `[YES]` opt-in (avoids the always-on `count(*)` cost Postgraphile incurs) | `[YES]` (default) | `[YES]` |
| Distinct on | `[NO]` targeted v0.2 | `[PARTIAL]` (smart tag) | `[YES]` |

## Mutations

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| Single-row insert | `[YES]` (`crates/magna-build/src/resolve/mutation.rs`) | `[YES]` | `[YES]` |
| Single-row update by primary key | `[YES]` | `[YES]` | `[YES]` |
| Single-row delete by primary key | `[YES]` | `[YES]` | `[YES]` |
| Returning fields after mutation | `[YES]` (uses `RETURNING`) | `[YES]` | `[YES]` |
| Bulk insert (`insert_many`) | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| Bulk update (`update_where`) | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| Bulk delete (`delete_where`) | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| Upsert / `ON CONFLICT` | `[NO]` targeted v0.2 | `[PARTIAL]` (smart tag) | `[YES]` |
| Nested writes (insert with relations) | `[NO]` targeted v0.5; design TBD | `[PARTIAL]` (smart tag) | `[YES]` |
| Mutation transactions (multi-mutation atomic) | `[YES]` request-scoped tx in `magna-serv` | `[YES]` | `[YES]` |
| Custom mutation via Rust plugin | `[YES]` `extend_mutation` hook | `[PARTIAL]` (plugin system) | `[NO]` |
| Custom mutation via webhook (Tier 3) | `[YES]` (`crates/magna-remote/`) | `[NO]` | `[YES]` (actions) |

## Authorization

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| Postgres RLS (row-level security) | `[YES]` connection runs as authenticated role; `auth.uid()` style helpers honored | `[YES]` | `[NO]` (Hasura bypasses RLS, uses its own permission DSL) |
| JWT validation (HS256 / RS256) | `[YES]` (`crates/magna-serv/src/jwt.rs`) | `[YES]` | `[YES]` |
| Role mapping from JWT claim to Postgres role | `[YES]` (`crates/magna-serv/src/rls.rs`) | `[YES]` | `[PARTIAL]` (uses Hasura roles, not pg roles) |
| Per-field authorization in Rust plugin | `[YES]` resolver short-circuits with error | `[PARTIAL]` (plugin system) | `[YES]` (permissions DSL) |
| CEL-based row policies via YAML | `[NO]` Tier 2c targeted v0.2; engine choice (`cel-rust` vs alternatives) deferred | `[NO]` (uses RLS) | `[YES]` (permissions DSL) |
| Anonymous / public role | `[YES]` configurable fallback role | `[YES]` | `[YES]` |
| Header forwarding to webhooks | `[YES]` (`crates/magna-remote/`) | `[NO]` | `[YES]` |
| Introspection gating in production | `[NO]` flag targeted v0.2 | `[YES]` | `[YES]` |

Note on philosophy: magna leans on Postgres RLS as the primary authorization mechanism because it composes with whatever your application already does at the database layer. The CEL policy DSL in v0.2 is a convenience for projects that don't want to write RLS, not a replacement. Hasura's permissions DSL has the opposite default — it bypasses RLS and asks you to re-express policies in its language. We don't.

## Subscriptions

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| GraphQL-over-WebSocket transport | `[YES]` (`magna-subscriptions`, `graphql-ws` protocol) | `[YES]` | `[YES]` |
| LISTEN / NOTIFY-driven subscriptions | `[YES]` | `[YES]` | `[NO]` |
| Polling-based subscriptions (no LISTEN) | `[NO]` targeted v0.5 | `[NO]` | `[YES]` (this is Hasura's only mode) |
| Live queries (auto-refresh on data change) | `[NO]` targeted v0.5 opt-in; expensive | `[YES]` (live queries plugin) | `[YES]` |
| Cursor-based streaming subscriptions | `[NO]` targeted v0.5 | `[NO]` | `[YES]` (streaming subscriptions) |
| Per-subscriber filtering (RLS applied) | `[YES]` subscriptions run under the subscriber's role | `[YES]` | `[YES]` |

## Observability

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| Structured logging (`tracing` crate) | `[YES]` | `[PARTIAL]` (Pino, JSON-only) | `[YES]` |
| OpenTelemetry traces — request granularity | `[YES]` | `[PARTIAL]` (community plugin) | `[YES]` |
| OpenTelemetry traces — planner-step granularity | `[NO]` targeted v0.2 | `[NO]` | `[PARTIAL]` |
| Prometheus metrics endpoint | `[NO]` targeted v0.2 | `[PARTIAL]` (community plugin) | `[YES]` |
| Health endpoint (`/health`) | `[YES]` (`crates/magna-serv/`) | `[YES]` | `[YES]` |
| Query complexity / cost analysis | `[PARTIAL]` depth/complexity limits hardcoded; configurable in v0.2 | `[PARTIAL]` (depth limit only) | `[YES]` |
| Rate limiting middleware | `[NO]` targeted v0.2 | `[NO]` (use a reverse proxy) | `[YES]` (cloud only) |
| Slow-query logging | `[YES]` threshold-configurable in `tracing` config | `[YES]` | `[YES]` |
| Request ID propagation | `[YES]` (`tower-http::request_id`) | `[YES]` | `[YES]` |

## Extension model

magna's differentiator is that all four extension tiers produce the same internal `Plugin` instance — one override replaces one field, adjacent fields stay auto-generated. Override priority: WASM > webhook > computed SQL > Rust plugin > auto-generated.

| Tier | magna v0.1 | Postgraphile equivalent | Hasura equivalent |
|---|---|---|---|
| Tier 1: Rust plugin trait | `[YES]` (`crates/magna-build/src/extension.rs`) | `[PARTIAL]` JS plugin system (steep undocumented cliff) | `[NO]` |
| Tier 2a: YAML field exposure | `[YES]` (`crates/magna-config/`) | `[YES]` (smart tags) | `[YES]` (metadata API) |
| Tier 2b: Computed fields via SQL expression | `[NO]` targeted v0.2 | `[YES]` | `[YES]` |
| Tier 2c: CEL-based authorization policies | `[NO]` targeted v0.2 | `[NO]` | `[YES]` (permissions DSL) |
| Tier 2d: Plugin toggles (camelCase, Relay, etc.) | `[YES]` | `[YES]` | `[PARTIAL]` |
| Tier 3: HTTP webhook resolvers | `[YES]` (`crates/magna-remote/`) | `[NO]` (use Tier 1 to call HTTP) | `[YES]` (actions, remote schemas) |
| Tier 4: WASM component plugins | `[NO]` targeted v0.5; WIT interface deliberately not locked at v0.1 | `[NO]` | `[NO]` |

A note on Postgraphile parity: Postgraphile's plugin system is genuinely powerful, but its public surface is largely undocumented and changes shape across major versions. magna's Tier 1 trait is a smaller surface published as `experimental` in v0.x and frozen at v1.0 — fewer hooks, but contracted ones. If the smaller surface turns out insufficient for `fw-resolvers` (magna's dogfooding consumer), each gap becomes a v0.2 input.

## Transport and deployment

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| HTTP server (Axum) | `[YES]` (`crates/magna-serv/`) | `[YES]` (Express / Fastify) | `[YES]` |
| GraphQL over HTTP (POST + GET) | `[YES]` | `[YES]` | `[YES]` |
| GraphQL over WebSocket (subscriptions) | `[YES]` | `[YES]` | `[YES]` |
| GraphiQL / Playground bundled | `[NO]` toggle targeted v0.2 | `[YES]` | `[YES]` |
| CORS middleware | `[YES]` (`tower-http::cors`) | `[YES]` | `[YES]` |
| TLS termination | `[OUT OF SCOPE]` use a reverse proxy | `[OUT OF SCOPE]` | `[OUT OF SCOPE]` |
| Docker image | `[YES]` (root `Dockerfile`, see `examples/quickstart/`) | `[YES]` | `[YES]` |
| Single-binary release (no runtime deps) | `[YES]` (Rust static binary) | `[NO]` (Node runtime required) | `[YES]` |
| Schema reload on DDL change | `[PARTIAL]` introspection cache invalidates; auto rebuild targeted v0.2 | `[YES]` (watch mode) | `[YES]` |
| MSRV pinned in CI | `[YES]` (`rust-toolchain.toml`) | n/a | n/a |

## Governance and license

| Capability | magna v0.1 | Postgraphile | Hasura |
|---|---|---|---|
| Permissive OSS license | `[YES]` dual MIT / Apache 2.0 | `[YES]` (MIT) | `[PARTIAL]` (Apache 2.0 core; enterprise features under BSL/source-available) |
| Per-crate stability labels | `[YES]` (`GOVERNANCE.md` API stability section) | `[NO]` | `[NO]` |
| Written license-durability commitment | `[YES]` (`GOVERNANCE.md`: no BSL/SSPL relicense; 12-month notice if ever changed) | `[NO]` (implicit) | `[NO]` (history of license shifts) |
| Public CHANGELOG | `[YES]` (`CHANGELOG.md`) | `[YES]` | `[YES]` |
| CLA-free contribution policy | `[YES]` (`CONTRIBUTING.md`) | `[YES]` | `[NO]` (CLA required) |
| Single-maintainer status acknowledged | `[YES]` (`GOVERNANCE.md` with succession plan) | `[NO]` (single maintainer, not formally acknowledged) | n/a (corporate-backed) |
| Public roadmap | `[YES]` (this file + design doc) | `[PARTIAL]` (issue labels) | `[YES]` |

## Deliberately not pursued

These features are technically achievable but are not on magna's roadmap. We name them explicitly so adopters don't waste time looking for them.

- **Remote schema stitching.** Hasura and Apollo Federation conflate "compose multiple GraphQL schemas" with "engine that talks to Postgres." magna stays in lane: one Postgres, one schema. If you need composition, run a federation gateway (Apollo Router, Cosmo) in front of magna and other engines. Tier 3 webhook resolvers cover the common "I need one external field" case; full schema stitching is deliberately not a magna concern.
- **Multi-database queries spanning DBs.** One magna instance, one Postgres connection. Cross-database queries require a federation layer; see above. We will not implement Postgres FDW-driven cross-DB resolution because it pushes complexity into the planner where it doesn't belong.
- **UI-based admin console.** Hasura's console is its biggest UX win and its biggest source of "metadata drift between staging and prod" pain. magna config lives in version control: `magna.yaml`, Rust plugins, or both. Diff in PRs, deploy with the rest of your code. No console.
- **Managed cloud offering.** magna is OSS-only. There is no hosted magna in this plan. Deploy it yourself with the bundled Dockerfile or a Rust binary; it has no phone-home, no licensing service, no telemetry.
- **ORM features.** magna does not produce entity classes, does not own migrations, does not track schema evolution beyond reload-on-DDL. Use sqlx, sea-orm, diesel, Prisma, Atlas — whatever your stack already runs.
- **GraphQL-first schema design.** magna generates GraphQL from Postgres, never the other way around. If you want GraphQL-first, async-graphql or Juniper are the right tools.

## How to read this matrix

**Column ordering.** magna is on the left because this is its repo. Postgraphile sits next to it because their feature set is the closest functional analog and the comparison is most informative. Hasura is third because its differing philosophy (its own permission DSL, its own metadata model) makes per-row comparison less precise.

**"Partial" entries** name what's missing inline and link to the v0.x version that fills the gap. If you find a row marked `[PARTIAL]` without that detail, please open an issue against this file — that is a documentation bug we want to know about.

**"Not supported" entries** fall into two categories:

1. *Tracking commitments with target versions* ("targeted v0.2" / "targeted v0.5"). The target version is best-effort, not contractual.
2. *"Out of scope"* — see "Deliberately not pursued" for the reasoning.

**Update obligation.** PRs that add, remove, or change feature support must update this matrix in the same diff. Reviewers should reject feature PRs that don't touch this file.

**Where competitors win.** Several rows credit Postgraphile or Hasura for capabilities magna doesn't have at v0.1. That is intentional: this matrix is an audit, not marketing. The v0.2 plan closes most of those gaps; the v0.5 plan closes the rest except for what's in "Deliberately not pursued."

**Where magna wins today.** The Rust single-binary deploy story, the four-tier extension gradient (especially Tier 1 Rust plugins for shops that want to extend in the language they already use), RLS-first authorization, dual MIT/Apache licensing with a written durability commitment, and per-crate stability labels are the five things magna v0.1 already does better than either alternative. The rest is roadmap.
