# Contributing to magna

Thanks for your interest. Magna is a small project run by a single
maintainer (see [`GOVERNANCE.md`](./GOVERNANCE.md)). The contribution
flow is intentionally lightweight.

## Workflow

1. Open an issue first if the change is non-trivial. Bugs, design
   questions, and "I want to add X" all belong in the issue tracker so we
   don't end up with a stranded PR.
2. Fork and branch off `main`. Branch names follow the convention
   `feat/<thing>`, `fix/<thing>`, `refactor/<thing>`, `docs/<thing>`.
3. Make your change. Keep diffs focused. One logical change per PR is
   easier to review and easier to revert if needed.
4. Run the test suite (`cargo test --workspace`). All tests must pass
   before review.
5. Run the linter (`cargo clippy --workspace --all-targets -- -D warnings`).
6. Run formatter (`cargo fmt --all`).
7. Open a PR against `main`. CI must pass.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/) format:

```
<type>(<scope>): <summary>

<optional body>
```

Types: `feat`, `fix`, `refactor`, `docs`, `chore`, `test`, `perf`, `ci`.
Scopes typically match crate names: `magna-build`, `magna-serv`, etc.

Examples:

```
feat(magna-build): add ExtensionContext::add_data method
fix(magna-serv): handle empty Authorization header without panic
docs: clarify Tier 1 plugin lifecycle in extension-guide.md
```

## What gets reviewed

- Architecture / design: does this change fit magna's surface area? Is
  the public API right? Will this break someone consuming magna in v0.x?
- Tests: every behavior change needs a test. Bug fixes need a regression
  test that fails before the fix. New features need at least one
  end-to-end test through `build_schema`.
- Documentation: if a feature shows up in user-facing behavior, it must
  show up in `README.md`, `docs/feature-matrix.md`, and `CHANGELOG.md` in
  the same PR. Reviewers will ask about all three.
- Stability impact: changes to crates labeled `stable` in
  `GOVERNANCE.md` get more scrutiny than changes to `experimental` ones.
  Breaking changes to stable crates require a CHANGELOG entry under
  `### Breaking`.

## License of contributions

Magna is dual-licensed under MIT and Apache 2.0. By submitting a
contribution (commit, patch, PR, suggestion that lands), you agree that
your contribution is licensed under those same terms. There is no CLA
and no copyright assignment. You retain the copyright on your work.

This is the standard Rust ecosystem model.

## Reporting security issues

Do not file a public issue for security vulnerabilities. Use GitHub's
private security advisory feature (Security tab on the repository) or
email the maintainer directly. See `SECURITY.md` for full details.
