# receipt for run 1-0001: ✗ failed

- workflow: release-cycle
- mode: default
- run: run-2026-08-21-0001
- cost: 1.54k tokens (1.2k in / 340 out) · CPTV: 770 tokens per task · 2 reroutes

- ✓ **criteria** 3/3 green, each command below
- ✓ **baseline** 0 regressions across 2 comparisons (suite `make test`, hash `22cc66aa7d26`)
- ✓ **scope** 4 files touched, none outside it
- ✓ **review** by 2 independent runners via `review` (claude-code, codex)
- ✓ **event chain** 40 events, hash-linked, replayable

### ✗ node `build` — `cargo build` exited 101: cannot find value `x`

### criteria

- ✓ **T001** `test -f hello.txt` exits 0
- ✓ **T002** `cargo test -p yunta-core` exits 0
- ✓ **T002** `cargo clippy --workspace -- -D warnings` exits 0

- `yunta verify 1-0001` — checks this run's evidence again
