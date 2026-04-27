# Magna Governance

This document describes how Magna is licensed, maintained, and evolved. It is
intended to be honest about the current state of the project (a single
maintainer, early days) while making durable commitments about the things that
must not change without notice (the license, the contribution model).

If something here conflicts with marketing copy elsewhere, this document wins.

---

## License

Magna is dual-licensed under either of:

- [MIT License](./LICENSE-MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE)

at your option. This is the standard Rust ecosystem dual-license used by
projects like Rust itself, tokio, axum, and serde. You may choose whichever
license fits your situation; you are not required to comply with both.

### License durability commitment

This is the most important paragraph in this document. Read it as a contract.

**Magna will always be released under permissive OSS licenses. No BSL, no
SSPL, no source-available relicensing. If that commitment ever needs to
change, it will be announced with 12 months' notice and a clear path for
community forks.**

The post-HashiCorp, post-Redis, post-Elastic landscape made one thing clear:
license rugpulls happen, and they happen to projects that started out
permissive. Magna's promise is that you can build on top of this engine
without expecting that promise to be revoked when the next funding round
arrives. There is no funding round.

The 12-month notice clause exists because no one can credibly promise
"forever" without an escape valve. What we can promise is that any change
will give the ecosystem enough runway to fork, mirror, and continue, with
the existing permissive bits frozen at a known-good revision. There will
be no surprise relicensings of past releases, ever.

---

## Maintainership

### Current state

Magna currently has a single maintainer:

- **Shane McGuirt**
- GitHub: [@srmcguirt](https://github.com/srmcguirt)
- Email: srmcguirt@gmail.com

That is the truth of the project right now. There is no foundation, no
steering committee, no advisory board, no fictional core team. Treating
single-maintainer projects as if they were Apache Software Foundation
incubator candidates is a failure mode this document refuses to enact.

### Succession plan

Single-maintainer projects need a continuity story, even if it's a small
one. Here is the plan.

**Contact paths.** If you need to reach the maintainer:

1. Open a GitHub issue at https://github.com/fellwork/magna/issues for
   anything code-related, security-related (use a private security
   advisory), or governance-related.
2. Email srmcguirt@gmail.com for anything that should not be public yet
   (CVE coordination, license questions, transfer requests).

GitHub issues are the canonical path. Email is the backup.

**Continuity policy.** If the maintainer is unreachable for 30 consecutive
days through both GitHub and email (no commits, no issue replies, no email
acknowledgement), the project is considered to be in a continuity event.
In that case:

- Anyone may fork the repository under the existing dual license. That is
  the whole point of permissive licensing.
- A trusted contributor (someone with a track record of merged PRs and
  visible engagement) may post an issue titled `continuity: maintainer
  unreachable` documenting attempts to reach the maintainer. After 60
  total days of unreachability, that issue serves as public notice that
  the upstream is paused.
- The maintainer will, on a best-effort basis, designate a successor in
  this file before any planned absence longer than 30 days. If no
  successor is named and the maintainer does not return, the community is
  free to coalesce around a fork; the upstream repository will not be
  deleted, and the npm/crates.io accounts will not be transferred to
  unknown parties.

This is not a perfect plan. It is the honest plan for a project of this
size.

**Governance review at v1.0.** When Magna reaches v1.0 and has sustained
external contribution (defined here as: at least three contributors other
than the maintainer with merged non-trivial PRs over a six-month window),
this governance document will be revisited. The intent is to formalize
something closer to a small maintainer team, with a documented decision
process for breaking changes, security response, and release cadence.
Until that bar is met, the honest answer is "Shane decides," and the
honest answer is the one this document gives.

---

## API stability

Magna ships as a workspace of crates. Each crate carries an explicit
maturity label, published in its README and respected by the release
process.

### Stability labels at v0.1

**Stable** (breaking changes require a minor-version bump on the 0.x
track, will be rare, and will be flagged in CHANGELOG):

- `magna-types`
- `magna-core`
- `magna-sql`
- `magna-introspect`
- `magna-dataplan`
- `magna-serv`

**Experimental** (breaking changes may land in any minor version on the
0.x track without a separate signal beyond the CHANGELOG entry):

- `magna-config`
- `magna-build`
- `magna-subscriptions`
- `magna-remote`

**The `magna` binary itself:** the CLI surface (subcommand names, flag
names, exit codes, stdout contract) is **stable**. The internal wiring
(how subcommands compose the underlying crates, plugin discovery, embedded
defaults) is **experimental** and may be reorganized between minor
versions on the 0.x track.

### Semver rules during 0.x

Cargo's semver rules treat 0.x as "anything goes," which is correct for
the language but unhelpful for downstream users trying to plan upgrades.
Magna applies tighter rules during the 0.x track:

- For **stable** crates, breaking changes require a minor-version bump
  (0.1.x to 0.2.0) and an entry in the relevant `CHANGELOG.md` under a
  `Breaking` heading. Patch releases (0.1.0 to 0.1.1) are bug-fix only.
- For **experimental** crates, breaking changes may land in minor
  versions without separate ceremony, but must still be documented in the
  `CHANGELOG.md`. Patch releases remain bug-fix only.
- The `magna` binary follows the **stable** rule for its CLI surface and
  the **experimental** rule for everything else.

### v1.0 commitment

When Magna reaches v1.0:

- All crates listed above move to **stable**.
- Any crate that cannot honestly meet the stable bar at v1.0 will either
  be cut from the v1.0 release or kept at a 0.x version under a clearly
  labeled "preview" status. v1.0 will not be a marketing milestone
  layered over experimental internals.
- Standard semver applies from that point forward: breaking changes
  require a major-version bump.

---

## Contributing

Magna is CLA-free. There is no Contributor License Agreement, no
copyright assignment, and no separate signature step.

By submitting a contribution (a pull request, a patch, a documentation
change, a code suggestion that lands in the tree), you agree that your
contribution is licensed under the same dual MIT / Apache 2.0 terms as
the rest of the project. You retain the copyright on your contribution;
you grant the project (and everyone downstream of it) the rights set out
in those licenses.

This is the same model used by most of the Rust ecosystem. It works
because permissive licenses already grant the rights a CLA would
otherwise extract, so a CLA is mostly ceremony.

For practical guidance on how to contribute (build setup, test commands,
PR conventions, code style), see [`CONTRIBUTING.md`](./CONTRIBUTING.md).

---

## Changes to this document

Changes to this governance document are themselves governed by it. The
license durability commitment in particular cannot be weakened without
the 12-month notice clause being honored. Other sections (succession
contact paths, stability labels, contributor list) are expected to evolve
and will be updated through normal pull requests.
