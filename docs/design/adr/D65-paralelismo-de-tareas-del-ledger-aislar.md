---
number: D65
title: "Paralelismo de tareas del ledger: aislar para trabajar, serializar para verificar (Contrato §5.5)"
status: accepted
revises: []
revised_by: []
---

# D65 — Paralelismo de tareas del ledger: aislar para trabajar, serializar para verificar (Contrato §5.5)

`concurrency: N` en el loop; lotes formados con tareas `ready` de scopes
disjuntos (la validación de scopes pasa de advertencia a criterio de
agrupamiento); un worktree por tarea del lote desde el commit base (sin esto,
dos agentes sobre un árbol se invalidan checks, scope y memoización
mutuamente); integración **de a una, en orden de declaración del ledger** (no
de finalización: el orden por timing no es reproducible y el mismo ledger debe
producir la misma secuencia de commits), con **rebase sobre el estado actual y
reejecución de criterios post-integración** — verde en el árbol individual es
necesario pero no suficiente. Guards y suites globales: una vez por lote
integrado, resuelto por la memoización sin lógica extra. Presupuesto reservado
por tarea al formar el lote. Resume sin caso especial: tareas huérfanas se
reejecutan, sus worktrees se descartan y renacen. **Default `concurrency:
1`**: el paralelismo multiplica el gasto simultáneo y nadie debe descubrirlo
por la factura; subirlo se decide con los datos de wall-clock de `stats`.

Descartados: trabajo paralelo con checks sobre árbol compartido (rompe scope y
memoización), integración por orden de finalización (no reproducible), y
aceptar el verde del árbol individual como veredicto final (miente cuando otra
tarea ya integró).
