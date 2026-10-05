---
number: D84
title: "Re-plan: una tarea conserva `done` solo si su identidad verificable no cambió (Contrato §5.7)"
status: accepted
revises: []
revised_by: []
---

# D84 — Re-plan: una tarea conserva `done` solo si su identidad verificable no cambió (Contrato §5.7)

Al reejecutarse un nodo de planificación, el ledger nuevo se contrasta con el
anterior tarea por tarea: sobreviven `done` únicamente las que mantienen `id`,
`criteria` y `scope` idénticos; cualquier diferencia devuelve la tarea a
`pending`.

Racional: `done` significa "sus criterios pasaron" — si los criterios
cambiaron, el done anterior no dice nada sobre la tarea nueva. El trabajo
commiteado no se revierte (las tareas invalidadas corren sobre el estado
actual y su pre-check en rojo dirá si seguían siendo necesarias), y todo el
rebalanceo se registra con `task_status_changed` citando el re-plan y una
línea en el recibo.

Descartados: descartar todo el ledger anterior (tira trabajo verificado) y
conservar por `id` a secas (un planificador puede reusar `T003` para algo
distinto y el engine lo daría por hecho — la garantía dependería de la
prolijidad del agente, contra el principio de que su palabra no es evidencia).
