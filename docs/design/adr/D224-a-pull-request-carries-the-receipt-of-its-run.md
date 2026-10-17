---
number: D224
title: "A pull request a run opens carries the run's receipt, and its marker is a comment"
status: accepted
revises: [D207]
revised_by: []
---

# D224 — A pull request a run opens carries the run's receipt, and its marker is a comment

## Context

A `pull_request` node opened a pull request whose body was what the author wrote,
then a line `run_id: `…``. The receipt — what the run held its work to and found —
was described as the thing to attach to a pull request, but nothing attached it:
`yunta receipt` refuses a run that has not finished, and the node that opens the
pull request is a step of the run. The reference pack's node set only a title, so
the body was the marker alone, and the marker sat in the text a reviewer reads.

## Decision

1. **The body carries the run's receipt by default**: after what the author wrote,
   the receipt as the log stands when the pull request opens — the run still open,
   its cost what a close now would record — drawn as Markdown by the forge
   adapter, the edge that writes the page. `receipt: false` on the node leaves
   only the author's body.
2. **The engine hands the receipt as data**; it does not draw it (D223).
3. **The run's marker is an HTML comment** at the end of the body
   (`<!-- yunta run_id: … -->`), which a forge's page does not show. A run still
   finds an open pull request carrying the earlier `run_id: `…`` line, so pull
   requests opened before keep being reused.

## Rationale

A reviewer reads the pull request, not the run's directory. The receipt is derived
from the log at any moment, so nothing waits for the run to finish to say what it
has proved; saying the run is still open keeps it honest about what a close may
add.

## Rejected alternatives

**The receipt only when the author wrote no body.** An author who wrote a summary
would lose the evidence beside it, and the choice would depend on prose.

**A comment posted after the run finishes.** It needs a step outside the run, and
the pull request would be reviewed first without it.
