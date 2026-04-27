# magna quickstart

A working GraphQL API over a tiny todos schema in five minutes. No code to
write. You start Postgres, point magna at it, and query.

## Prerequisites

- Docker 24+ (or Podman with `docker` alias)
- A free terminal on ports `5544` and `4800`

If you would rather run without Docker, you need Rust 1.83+ and a local
Postgres 16. Substitute step 1 with `psql -f seed.sql` against your own
database and skip the docker network bits.

## What you will build

A GraphQL endpoint exposing four tables: `users`, `todos`, `tags`,
`todo_tags`. magna introspects Postgres at startup and generates queries
(`allUsers`, `userById`, `allTodos` with filters and ordering) and mutations
(`createTodo`, `updateTodo`, `deleteTodo`) automatically. No schema files,
no resolvers.

The whole thing is four shell commands.

## Step 1: start Postgres with the seed schema

From this directory:

```bash
docker run -d \
  --name magna-quickstart-db \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=magna_demo \
  -p 5544:5432 \
  -v "$(pwd)/seed.sql:/docker-entrypoint-initdb.d/seed.sql:ro" \
  postgres:16
```

The mounted `seed.sql` runs once on first boot. Wait about three seconds
for the database to be ready, then verify:

```bash
docker exec magna-quickstart-db psql -U postgres -d magna_demo -c "select count(*) from todos;"
```

You should see `7`.

## Step 2: run magna

```bash
docker run --rm -d \
  --name magna-quickstart \
  --add-host=host.docker.internal:host-gateway \
  -e DATABASE_URL="postgres://postgres:postgres@host.docker.internal:5544/magna_demo" \
  -p 4800:4800 \
  ghcr.io/fellwork/magna:latest
```

The default mode runs magna with no `SchemaExtension` instances, so the
GraphQL schema is whatever auto-CRUD falls out of introspection. Tail the
logs to confirm the schema built:

```bash
docker logs magna-quickstart
```

Look for `introspected 4 tables` and `listening on 0.0.0.0:4800`.

## Step 3: query it

GraphiQL is served at `http://localhost:4800/graphql`. Open it in a browser
and paste a query from `queries.md`, or use curl:

```bash
curl -s http://localhost:4800/graphql \
  -H 'content-type: application/json' \
  -d '{"query":"{ allUsers { nodes { id email } } }"}' | jq
```

Create a new todo:

```bash
curl -s http://localhost:4800/graphql \
  -H 'content-type: application/json' \
  -d '{"query":"mutation { createTodo(input: { todo: { title: \"buy milk\", userId: \"11111111-1111-1111-1111-111111111111\" } }) { todo { id title done } } }"}' | jq
```

Fetch a single todo by id:

```bash
curl -s http://localhost:4800/graphql \
  -H 'content-type: application/json' \
  -d '{"query":"{ todoById(id: \"aaaaaaa1-0000-0000-0000-000000000001\") { title done dueDate } }"}' | jq
```

See `queries.md` in this directory for filtering, ordering, pagination, and
joined examples.

## Step 4: inspect the generated SDL

magna serves the introspected schema as SDL on a sidecar route:

```bash
curl -s http://localhost:4800/graphql/sdl
```

Pipe it to a file if you want to keep it:

```bash
curl -s http://localhost:4800/graphql/sdl > schema.graphql
```

This is the same SDL the GraphiQL endpoint is built from. Useful for
codegen on the client side.

## Cleanup

```bash
docker stop magna-quickstart magna-quickstart-db
docker rm magna-quickstart-db
```

The Postgres container had no named volume, so its data is gone with the
container. If you re-run step 1, the seed runs fresh.

## Troubleshooting

**`DATABASE_URL` connection refused.** The magna container cannot reach
`localhost` on the host. The `--add-host=host.docker.internal:host-gateway`
flag is what makes `host.docker.internal` resolvable on Linux. On macOS and
Windows Docker Desktop it works without the flag, but adding it is
harmless. Alternatively put both containers on a user-defined network:

```bash
docker network create magna-net
# add --network magna-net to both runs above
# then DATABASE_URL becomes postgres://postgres:postgres@magna-quickstart-db:5432/magna_demo
```

**`no schema found` or `introspected 0 tables`.** The seed did not run.
This happens if the Postgres data directory was already initialized from a
previous run. Remove the container (`docker rm -f magna-quickstart-db`)
and re-run step 1. Confirm the mount path in the `docker run` command
points at the absolute path of `seed.sql` on your host. On Windows
PowerShell, replace `$(pwd)` with `${PWD}`; on cmd.exe, use `%cd%`.

**Port 4800 already in use.** Pass `-p 4801:4800` and hit
`http://localhost:4801/graphql` instead. The container still listens on
4800 internally; only the host mapping changes.

**Mutations return `permission denied`.** magna respects Postgres roles.
The quickstart connects as the `postgres` superuser so writes work. If you
swap in a read-only role, only queries will resolve.
