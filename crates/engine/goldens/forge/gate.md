◆ **decision** · needs you · gate-1

Gate `approve-plan` waits on this pull request, for lead.

## plan: 1 task

Make it.

Writes the file the run is about.

## steps

| step | task | what it does |
|---|---|---|
| 1 | `T001` | Make it |

## step 1 of 1

<details>

<summary><b>T001</b>: Make it</summary>

- you will see: made.txt exists
- changes: made.txt
- tested by: the file exists  
  `test -f made.txt`

`made.txt`: the file

```
the file's first line
```

_1 line_

</details>

## ◆ Is this plan the one to build?

`.yunta/gates/run-gate-1/approve-plan/tasks.md`: the whole plan, each task with its code and what proves it  
`.yunta/gates/run-gate-1/approve-plan/tasks.yaml`: as the run holds it

| answer | on GitHub | what happens |
|---|---|---|
| **approve** | approve this pull request, or merge it | resolves this gate; nothing in the workflow waits on it |
| **request changes** | request changes, saying what to change in comments | each comment reaches `plan` as a finding; it runs again, what it makes lands on this pull request, and the gate asks again here |
| **close** | close this pull request | nothing runs; this decision stays open |

on the machine that holds the run

- `yunta resume gate-1`: reads the answer and acts on it
- `yunta close gate-1`: closes the run for good

<!-- yunta run_id: run-gate-1 -->