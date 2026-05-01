# magna

Rust GraphQL engine that auto-generates from Postgres, extends like a normal
Rust app, and is governed like you can trust it. Public, dual-licensed
(MIT/Apache-2.0). The fw-graph engine moved here from the api workspace.

## Commands

```bash
cargo check --workspace      # Compile-check everything
cargo test --workspace       # All tests
cargo build --release        # Production build
cargo run -- --help          # Single-binary CLI usage
cargo deny check             # Supply-chain audit (vendored licenses, advisories)
```

## Stack

- Rust 1.83+ (see `rust-toolchain.toml`)
- Workspace crates under `crates/`
- `cargo-deny` for supply-chain governance
- Per-crate stability labels (`stable` / `experimental`) — see `GOVERNANCE.md`

## Conventions

- **License durability commitment:** never re-license away from MIT/Apache-2.0.
- **Stability labels matter.** A breaking change in a `stable` crate needs a SemVer major bump and `CHANGELOG` entry. Read `GOVERNANCE.md` before changing a public API.
- Library crates use `thiserror`, never `anyhow`. Never `.unwrap()` in library code — return `Result<T, E>`.
- Use `tracing` for structured logging.
- Honest stability — no marketing-grade "production ready" claims.

## Phase 1 (Magna integration)

22 Fellwork Query fields are wired via `FellworkExtension` (live-verified
against local DB on `feat/split-fw-resolvers` at 0df6960, not yet pushed).

## gstack

AI dev tooling — headless browser, QA, design review, deploy workflows.

**Install (one-time per machine):**
```bash
git clone --single-branch --depth 1 https://github.com/garrytan/gstack.git ~/.claude/skills/gstack && cd ~/.claude/skills/gstack && ./setup
```

Use `/browse` for all web browsing. Never use `mcp__claude-in-chrome__*` tools directly.

Available skills:
`/office-hours`, `/plan-ceo-review`, `/plan-eng-review`, `/plan-design-review`, `/design-consultation`, `/design-shotgun`, `/design-html`, `/review`, `/ship`, `/land-and-deploy`, `/canary`, `/benchmark`, `/browse`, `/connect-chrome`, `/qa`, `/qa-only`, `/design-review`, `/setup-browser-cookies`, `/setup-deploy`, `/setup-gbrain`, `/retro`, `/investigate`, `/document-release`, `/codex`, `/cso`, `/autoplan`, `/plan-devex-review`, `/devex-review`, `/careful`, `/freeze`, `/guard`, `/unfreeze`, `/gstack-upgrade`, `/learn`

## Agent-Specific Notes

This repository includes a compiled documentation database/knowledgebase at `AGENTS.db`.
For context for any task, you MUST use MCP `agents_search` to look up context including architectural, API, and historical changes.
Treat `AGENTS.db` layers as immutable; avoid in-place mutation utilities unless required by the design.
