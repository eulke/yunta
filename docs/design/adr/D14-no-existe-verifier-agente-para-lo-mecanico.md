---
number: D14
title: "No existe verifier-agente para lo mecánico"
status: revised
revises: []
revised_by: [D174]
---

# D14 — No existe verifier-agente para lo mecánico

*(Revisada por D174: no hay juicio LLM de completitud; `manual_review` y
`justification` se retiran, y una tarea que un comando no puede cerrar es un
`gate`.)* La verificación es una fase del engine, no un rol. Único juicio LLM:
`manual_review: true` acotado a nodo de auditoría con rúbrica.

Descartado: separación executor/verifier como dos agentes que respetan reglas.
