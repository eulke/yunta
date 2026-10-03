---
number: D19
title: "ContextSource como trait"
status: revised
revises: []
revised_by: [D157, D167, D186]
---

# D19 — ContextSource como trait

*(Revised by D186: a `files:` entry declared `optional: true` that is missing
is not a failed source; the session gets a marker and `context_assembled`
records the absence.)* *(Revisada por D167: las fuentes propias por executor quedan registradas como
deuda A-15; `ContextSpec` es cerrada.)* *(Revisada por D157: la fuente
`artifact` resuelve por el log del run y toma los bytes del store, nunca por
el directorio; la fuente `ledger` se llama `tasks`.)* con builtins `files`,
`command`, `artifact`, `mcp`, `run-events`, `ledger`, `knowledge`,
`node-output`; fuentes propias vía executors. Resolución previa a la sesión,
materializada con hash y evento; fuente caída = fallo del nodo.
