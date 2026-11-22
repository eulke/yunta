# Agent docs

What the code cannot say about itself, for an agent that changes Yunta. `AGENTS.md` at the root sends a session
here. A person using Yunta reads `docs/` and the README; nothing here is for them.

- `architecture.md`: how the crates fit, each product rule in one line tagged with what holds it (with `Paths:`
  when only review does), and the guarantees no single module owns. Read it before you plan or review a change,
  change what a session is told, or add a pause, a gate, per-run work, text for a person, an adapter feature or
  a crate edge.
- `choices.md`: what Yunta does not do, and why. Read it before you propose a mechanism, a config key or a
  command.
- `open.md`: work not done yet and questions not answered, by the paths they touch. Read it before you take on
  a gap. The change that does an entry deletes it.
- `traps.md`: mistakes that cost a cycle and that no check and no single line can hold, by the paths where they
  bite. Read it before you write a test. The change that adds a trap's check deletes it.

`cargo xtask docs --about <files>` prints what these files and the tests record about the paths you name. Where
it cannot build (a run's planner, spec writer or reviewer; plan mode), read `traps.md` and the product rules
whole, and run `git grep -n 'Reached by:' -- crates xtask`.

`cargo xtask docs --check` holds the tree to these five files and each to its budget and shape.
