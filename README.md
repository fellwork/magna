# magna

[![Rust](https://img.shields.io/badge/rust-1.83%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE-APACHE)
[![CI](https://img.shields.io/github/actions/workflow/status/fellwork/magna/ci.yml?branch=main&label=CI)](https://github.com/fellwork/magna/actions)
[![Stability](https://img.shields.io/badge/status-technical%20preview-yellow.svg)](GOVERNANCE.md)

> Rust GraphQL engine that auto-generates from Postgres, extends like a normal Rust app, and is governed like you can trust it.

## What it does

- Auto-generates a GraphQL API from any Postgres schema. Tables, views, functions, enums, and foreign-key relationships become queries, mutations, and connection types with no hand-written resolvers.
- Four-tier extension surface so customization is field-level, not schema-level. A Rust trait (`SchemaExtension`), declarative YAML, HTTP webhooks, and (in v0.5) WASM components all produce the same plugin shape internally.
- Ships as a single binary or as a stack of Rust crates. Run `magna` against a `DATABASE_URL` and you have a server. Pull `magna-build` and `magna-serv` into your own binary if you need to embed.
- Honest stability labels per crate. `stable`, `experimental`, and what changing each one means for your build are spelled out in `GOVERNANCE.md`. No marketing-grade "production ready" claims that crumble in a minor release.
- Permissive dual license, MIT or Apache 2.0, with a written license-durability commitment. No BSL, no SSPL, no source-available bait-and-switch.

## Why magna exists

The auto-generated GraphQL category fumbles three things in the same way.

Postgraphile is brilliant work, but extension lives behind magic SQL comments ("smart tags") or a plugin system with a steep undocumented cliff. It's single-maintainer, and the JS/Node tax is real for shops that don't otherwise run Node. Hasura's licensing and governance arc (source-available gates, the DDN rewrite, "talk to sales" enterprise pricing) has eroded trust, and its YAML permission DSL doesn't compose with the auth decisions your application already makes. In both, customization is take-it-or-leave-it at the table or schema level. You can't smoothly override one field while keeping everything around it auto-generated.

Magna's bet is that customization should be a gradient. The auto-generated default is one node in a precedence chain. A Rust plugin, a YAML computed field, an HTTP webhook, or a WASM component can replace exactly one field, and the fields next to it stay auto-generated. The same priority chain applies whether you're overriding `Query.weather` with a remote API or rewriting `User.fullName` in Rust.

Governance is the other bet. Magna is solo-maintained today and says so plainly in `GOVERNANCE.md`, with a written succession plan, per-crate stability labels, and a public commitment that the license stays permissive. That's not a foundation-transition fantasy. It's the minimum a serious adopter should expect after the last few years of license drama.

## 5-minute quickstart

You'll need Docker and `curl`.

```bash
# 1. Start a Postgres with a tiny seed schema.
docker run -d --name magna-pg \
  -e POSTGRES_PASSWORD=postgres \
  -p 5432:5432 \
  postgres:16

docker exec -i magna-pg psql -U postgres <<'SQL'
create table author (
  id   bigserial primary key,
  name text not null
);
create table post (
  id        bigserial primary key,
  author_id bigint references author(id),
  title     text not null,
  body      text
);
insert into author (name) values ('Ada Lovelace'), ('Grace Hopper');
insert into post (author_id, title, body) values
  (1, 'On the Analytical Engine', 'Notes on Menabrea...'),
  (2, 'On Compilers',             'A few thoughts on A-0.');
SQL

# 2. Run magna against it.
docker run --rm -p 4800:4800 \
  -e DATABASE_URL=postgres://postgres:postgres@host.docker.internal:5432/postgres \
  ghcr.io/fellwork/magna:0.1

# 3. Hit the GraphQL endpoint.
curl -s http://localhost:4800/graphql \
  -H 'content-type: application/json' \
  -d '{"query":"{ allAuthors { nodes { id name } } }"}'
```

You should see the two authors with no resolver code written. The same pattern works against your real schema, only larger.

A more complete walkthrough lives in `examples/quickstart/`, including a docker-compose setup, a YAML config that renames `author` to `Account`, and a worked Rust extension that adds a `Query.healthcheck` field.

## Extension model

Four tiers, one priority chain. For any field `Type.field`, the resolution order is WASM, then HTTP webhook, then computed SQL, then Rust plugin, then the auto-generated default.

| Tier | Mechanism                                                | v0.1 status     |
| ---- | -------------------------------------------------------- | --------------- |
| 1    | Rust trait `SchemaExtension` with `register_type`, `query_field`, `mutation_field`, `add_data` | shipping        |
| 2a   | YAML field exposure (rename, hide, expose FKs)           | shipping        |
| 2b   | YAML computed fields (inline SQL)                        | targeted v0.2   |
| 2c   | YAML CEL authorization policies                          | targeted v0.2   |
| 3    | HTTP webhook resolvers via `magna-remote`                | shipping        |
| 4    | WASM component plugins via `magna-wasm` (WIT interface)  | targeted v0.5   |

A minimal Tier 1 extension looks like:

```rust
use magna_build::{ExtensionContext, SchemaExtension};
use async_graphql::dynamic::{Field, FieldFuture, TypeRef};

pub struct Healthcheck;

impl SchemaExtension for Healthcheck {
    fn name(&self) -> &str { "healthcheck" }

    fn extend_query(&self, ctx: &mut ExtensionContext<'_>) {
        ctx.query_field(Field::new(
            "healthcheck",
            TypeRef::named_nn(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(async_graphql::Value::from("ok"))),
        ));
    }
}
```

A minimal Tier 2a config looks like:

```yaml
tables:
  users:
    expose: true
    rename_as: Account
    omit_fields: [password_hash]
    expose_fk_as: { org_id: organization }
  internal_audit: { expose: false }
```

A minimal Tier 3 webhook looks like:

```yaml
remote_resolvers:
  - field: Query.weather
    url: "https://weather.example.com/resolver"
    args_schema: { city: String }
    returns: WeatherReport
```

The full guide, including how the four tiers compose and where the gradient breaks down, is in `docs/extension-guide.md` (forthcoming).

## Status and stability

**v0.1 is technical-preview quality.** It's the same engine that powers a private Bible-reading app in production, extracted with full git history, but the public API has not yet absorbed feedback from anyone outside the original use case. Expect breaking changes in `experimental` crates on minor versions through v0.5. v1.0 freezes the API.

Per-crate labels:

| Crate                | Role                                              | Stability      |
| -------------------- | ------------------------------------------------- | -------------- |
| `magna-types`        | Shared types (StepResult, PgValue, errors)        | stable         |
| `magna-core`         | Two-phase planner and executor                    | stable         |
| `magna-sql`          | Composable SQL AST builder                        | stable         |
| `magna-introspect`   | pg_catalog discovery, NOTIFY-driven cache         | stable         |
| `magna-dataplan`     | Postgres data-plan steps                          | stable         |
| `magna-config`       | Plugin trait, presets, policies                   | experimental   |
| `magna-build`        | Auto-CRUD resolvers, schema builder               | experimental   |
| `magna-serv`         | Axum server, JWT, plan cache                      | stable         |
| `magna-subscriptions`| LISTEN/NOTIFY subscriptions                       | experimental   |
| `magna-remote`       | HTTP webhook resolvers                            | experimental   |
| `magna` (binary)     | Top-level binary                                  | stable CLI, experimental internals |

What "stable" and "experimental" actually mean for your `Cargo.toml` is in `GOVERNANCE.md`. The honest "supported / partial / unsupported" comparison vs Postgraphile and Hasura is in `docs/feature-matrix.md`.

## Documentation

- [`GOVERNANCE.md`](./GOVERNANCE.md) — license commitment, succession plan, per-crate stability rules.
- [`CHANGELOG.md`](./CHANGELOG.md) — what shipped when, in Keep a Changelog format.
- [`docs/feature-matrix.md`](./docs/feature-matrix.md) — what works today vs what's targeted, with links to implementing code.
- `docs/architecture.md` — engine internals, two-phase planner, dataplan steps. (Forthcoming.)
- `docs/extension-guide.md` — all four tiers, with worked examples. (Forthcoming.)
- [`examples/quickstart/`](./examples/quickstart/) — copy-paste runnable starter.
- `examples/plugin-rust/` — Tier 1 SchemaExtension end-to-end. (Forthcoming.)
- `examples/yaml-config/` — Tier 2a field exposure end-to-end. (Forthcoming.)
- [`CONTRIBUTING.md`](./CONTRIBUTING.md) — PR process, no CLA.

## Crate layout

```
crates/
  magna-types/           shared types, errors
  magna-core/            two-phase planner and executor
  magna-sql/             composable SQL AST builder
  magna-introspect/      pg_catalog cache with NOTIFY invalidation
  magna-dataplan/        Postgres data-plan steps
  magna-config/          plugin trait, presets, YAML loading
  magna-build/           auto-CRUD resolvers, schema builder, SchemaExtension trait
  magna-serv/            Axum server, JWT auth, RLS context, plan cache
  magna-subscriptions/   LISTEN/NOTIFY-driven subscriptions
  magna-remote/          HTTP webhook resolvers (Tier 3)
  magna/                 top-level binary
```

## Building from source

```bash
git clone https://github.com/fellwork/magna
cd magna
cargo test --workspace
cargo run -p magna -- --help
```

Minimum supported Rust version is pinned in `rust-toolchain.toml`. CI builds against it on every PR.

## License

Dual-licensed under either of:

- MIT license (`LICENSE-MIT`)
- Apache License 2.0 (`LICENSE-APACHE`)

at your option. Contributions are accepted under the same dual license. There is no CLA. The license-durability commitment in `GOVERNANCE.md` says, in writing, that magna will always be released under permissive OSS licenses, with 12 months' notice and a clear fork path if that ever needs to change.

Maintained by Shane McGuirt. See `GOVERNANCE.md` for contact paths and the succession plan.
