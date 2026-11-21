# Traps

Mistakes that cost a cycle here and that no check holds yet: `cargo xtask docs --about <paths>` prints the
entries whose paths match a file you name, and the change that adds a check for an entry deletes it.

### A test that claims concurrency
Paths: `crates/*/tests/**`, `crates/testkit/src/**`, `crates/engine/src/run_tools/watch.rs`,
`crates/engine/src/run_tools/listener.rs`, `crates/engine/src/process_registry.rs`,
`crates/engine/src/run/loop_exec/batch.rs`
Wrong: an ordering assertion that a sequential implementation also passes.
Right: make the two sides wait on each other, so the test fails when the work does not overlap.

### A doc comment on a unit variant reshapes a schema
Paths: `crates/core/src/**`, `crates/core/schemas/**`
Wrong: a `///` on each variant of a unit enum a schema publishes: schemars then writes one `const` branch per
variant instead of the enum's `enum` list, a new shape for every reader of the schema.
Right: explain the variants in the enum's own `///`, and compare a regenerated schema with its descriptions
taken out before keeping it.

### A generic reader named in a second crate
Paths: `crates/*/src/**`
Wrong: `yaml::parse::<T>` or `PersistedDoc::read` on a manifest or a config layer outside core: a release
build shares no generics, so each crate that names one builds its own copy of every deserializer `T` holds,
and the static binary passed its size ceiling that way.
Right: read it through core's concrete reader (`Manifest::read_frozen`, `ConfigLayer::read`), and give a
document a second crate reads one of its own.

### A run refused for room on a full disk
Paths: `crates/*/tests/**`, `crates/testkit/src/**`, `crates/testkit-ui/src/**`, `crates/engine/src/worktree/pool/room.rs`
Wrong: reading a `NoRoom` refusal in a test, a recording or a real run as the change's fault: the pool keeps the
larger of a twentieth of the disk and 10 GB free on the disk the test runs on, and a build beside it can take that.
A test that waits for what the refused run never starts hangs with no bound (`crates/cli/tests/session_ends.rs`
waits for a session's process), so a suite under that floor may never end.
Right: read the disk's free space first, against that floor and not a rounder number; make room (a `cargo clean`
of your own checkout, and the builds a pool keeps under `~/.yunta/worktrees/pool/` with its owner's leave) and run
again.

### A test file under `src` that the smell counters read as production
Paths: `crates/*/src/**/tests/*.rs`, `xtask/src/smells/corpus.rs`
Wrong: taking a file under `src/**/tests/` for a test file: the counters know a unit test's file by its name,
`tests.rs`, so a submodule of it (`src/chronicle/tests/feed.rs`) is counted as production code, and an assertion
that quotes a run tool's name or a long test function there raises a production counter.
Right: read `cargo xtask smells --check` after adding one; say what the assertion holds through the crate's own
values rather than a quoted literal the counter reads, and keep its functions under the production counts.

### A source put back with its old time is not built again
Paths: `crates/*/src/**`, `crates/*/tests/**`
Wrong: breaking a rule to see a test fail, then restoring the file from a copy that keeps its modification
time (`cp -p`, a backup moved back): cargo compares times, takes the file as unchanged since the broken build,
and the next run still holds the break.
Right: undo the edit in place, or `touch` the file after restoring it, and see the test pass again before
trusting any other result.
