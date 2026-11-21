# Brief: install a pack globally

A person installs a pack once for every project of theirs, beside the
project-level installs that exist today.

## Requirements

- A global pack lives in the current user's platform data directory,
  shared by that user's projects.
- `--global` on `pack add`, `update`, `remove` and `list` selects the
  global installation; without it, each command keeps its project
  behavior.

## Open points

- How global installations interact with project-level packs: nobody
  answered. Assumed: independent stores and locks; each operation
  affects the selected target.
