---
number: D87
title: "`executor` significa una sola cosa: extensión de código"
status: accepted
revises: []
revised_by: []
---

# D87 — `executor` significa una sola cosa: extensión de código

Se elimina el tercer uso informal de la palabra, que en la prosa del Contrato
nombraba al agente que ejecuta una tarea del ledger. `executor` queda
reservado para la extensión externa con contrato JSON (D47), en sus dos
apariciones coherentes: `kind: executor` (el nodo que la invoca) y
`skills.executors` (donde se declara). Quien hace el trabajo de una tarea es
**el runner de la tarea** o "el nodo de implementación", nombres que el
vocabulario ya tenía. Sin cambios de schema — solo prosa. Misma regla aplicada
a runner/agente/adapter (D27) y a gate/check (D85): una palabra, un
significado.
