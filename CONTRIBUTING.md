# Contributing to Yunta

Yunta is a deterministic workflow engine: state is derived by replaying an
append-only event log, and a workflow's own `criteria` are the only thing
allowed to mark work done — never an agent's word, and never a contributor's
either. That principle extends to how the project itself is built.

## Before you start

For anything beyond a small, obviously-scoped fix, open an issue first
describing the problem and your proposed approach. Design decisions in this
engine follow from a small set of hard invariants (see
[`docs/guide.md`](docs/guide.md)) — if your change would touch one of those,
the issue discussion is where that gets worked out, not the PR.

## Building and testing

```bash
cargo build --workspace
cargo test --workspace                    # full suite — always the mock adapter, never a real LLM
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p yunta -- check <workflow>    # static validation
cargo run -p yunta -- run <workflow> --adapter mock   # run without a real LLM
```

All four of the first commands must be clean before you open a PR. CI runs
the workspace test suite, an isolated `cargo check` per crate, clippy, fmt,
and `cargo deny check` — a crate that only compiles as part of the whole
workspace has a dependency in the wrong place, not a CI gap.

## Code it's asking you to write

- **Test-first, in red.** Write or point at the test that fails before your
  change exists. A test that already passes without the fix proves nothing.
- **No `unwrap()`/`expect()` outside tests.** Errors are typed (`thiserror`)
  and say what to do about them, not just that something went wrong.
- **`#![forbid(unsafe_code)]`** applies to every crate. There's no case in
  this codebase that needs an exception.
- **Newtypes over bare `String`/`u64`** for identifiers (`RunId`, `NodeId`,
  `TaskId`, ...), and exhaustive enums over boolean flags for state machines.
  Invalid states should fail to compile, not fail a runtime check.
- **Dependencies land in the crate that needs them**, never in the whole
  workspace or in `yunta-core` "to have it handy". `yunta-engine` doesn't
  depend on a storage backend or a concrete CLI, and `crates/{core, storage,
  adapters, engine, cli}` depend only downward — a new dependency that
  wants to go the other way means the design is wrong, not the layout.
- **Terminology is fixed**: `adapter` (never "driver"/"backend"), `runner`
  (never "role" as a YAML key — "role" is fine as prose), `agent` (never
  "subagent"), `pack` (never "plugin"), `executor` (never "plugin"). This
  matters because packs and adapters are meant to be written by people who
  aren't the maintainers, and the vocabulary is the contract between them.

## Commits and PRs

Conventional commit messages (`feat:`, `fix:`, `test:`, `docs:`,
`refactor:`), one topic per commit — the body explains *why* when that isn't
obvious from the diff. Keep PRs scoped to one change; a PR that "needs" to
touch half the tree is a sign the change is cut wrong, not a reason to
widen the review.

## Reporting a security issue

Don't open a public issue for a vulnerability — see [`SECURITY.md`](SECURITY.md).
