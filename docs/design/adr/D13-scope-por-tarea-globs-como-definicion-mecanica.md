---
number: D13
title: "Scope por tarea (globs) como definición mecánica de drift"
status: revised
revises: []
revised_by: [D172]
---

# D13 — Scope por tarea (globs) como definición mecánica de drift

*(Revisada por D172: el enforcement en caliente es el cerco — un juez en core,
un nivel por adapter, una cobertura por sesión y el kind `write_refused`;
`edit_hooks` desaparece.)* Post-check garantizado por diff; enforcement en
caliente vía capability `edit_hooks`. Scopes disjuntos = paralelismo
verificable. Hallazgos fuera de scope → `finding_posted`, nunca parche
silencioso.
