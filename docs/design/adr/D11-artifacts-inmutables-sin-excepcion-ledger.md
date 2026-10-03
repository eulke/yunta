---
number: D11
title: "Artifacts inmutables sin excepción; ledger de tareas en el event log"
status: revised
revises: []
revised_by: [D157]
---

# D11 — Artifacts inmutables sin excepción; ledger de tareas en el event log

*(Revisada por D157: también la definición del documento vive en eventos —
`artifact_accepted` con su hash —, y no en un archivo que se abre por
nombre.)* El estado de tareas vive como eventos (`task_status_changed`), no en
archivo mutable: no hay nada que adulterar, no hay nada que custodiar.

Descartado: archivo de tareas mutable protegido por convención/hash/script.
