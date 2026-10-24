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
