# Agent docs

What an agent reads before changing Yunta, beside the code: `AGENTS.md` at the root is the map that routes to
these files. A person using Yunta reads `docs/` and the README; nothing here is for them.

- `architecture.md` — how the crates fit, the product rules and what holds each.
- `choices.md` — what Yunta does not do, and why.
- `open.md` — work not done yet, by the paths it touches.
- `traps.md` — mistakes that cost a cycle, by the paths where they bite.

Each file is rewritten in place: a fact that became false is deleted, never marked. `cargo xtask docs --check`
holds the tree to these five files and each to its budget and shape.
