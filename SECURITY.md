# Security Policy

## Reporting a Vulnerability

Report security vulnerabilities privately through
[GitHub's private vulnerability reporting](https://github.com/eulke/yunta/security/advisories/new)
on this repository, rather than as a public issue. This opens a private
discussion with maintainers where you can share details (and a
proof-of-concept, if you have one) without exposing it before a fix ships.

Include, if you can:

- The version (or commit) you tested against.
- Steps to reproduce, or the workflow/config that triggers it.
- What you'd expect the engine to do instead (refuse, degrade explicitly,
  isolate the blast radius) and what it does today.

You should get an initial response within a few days. There's no bug bounty
program; credit in the eventual advisory is offered by default unless you
ask to stay anonymous.

## Scope

Yunta runs local workflows against a local SQLite event log and spawns
adapter processes (coding-agent CLIs) under a git worktree. Reports that are
particularly relevant to that shape:

- Anything that lets a workflow or pack escape its declared permissions
  ceiling (`declares:`/`permissions:`, see the [workflow guide](docs/guide.md))
  or execute outside its `scope`.
- Anything that leaks a secret into the event log, an adapter event payload,
  or a fixture — those are invariants the engine is supposed to hold
  unconditionally.
- Anything that lets a run's own state be forged or replayed into an
  inconsistent result (the event log is the sole source of truth for
  everything `status`/`resume`/`verify` report).
- Supply-chain issues in a published artifact (crates.io, the release
  binaries, the container image, the Homebrew formula) — a checksum
  mismatch, a compromised build step, or a signing gap.

General bugs that don't have a security impact belong in a regular issue.

## Supported Versions

Until the first tagged release, only the `main` branch is supported. Once
releases exist, see the compatibility policy (`docs/compatibility.md`) for
how many versions back get security fixes.
