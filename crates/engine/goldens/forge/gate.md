Gate `approve-plan` of run `run-gate-1` waits on this pull request, for lead.

### how to answer

- **approve** — approve this pull request, or merge it  
  the gate passes
- **request changes** — request changes, saying what to change in comments  
  each comment reaches `plan` as a finding; it runs again, what it makes lands on this pull request, and the gate asks again here
- **close** — close this pull request  
  the gate fails, and the run goes no further past it

### what it decides on

`tasks.md` — the plan, task by task, with what proves each one  
`tasks.yaml` — as the run holds it

### on the machine that holds the run

- `yunta resume gate-1` — reads the review and goes on from it
- `yunta cancel gate-1` — stops the run instead

<!-- yunta run_id: run-gate-1 -->