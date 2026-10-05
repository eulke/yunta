---
number: D45
title: "Progreso observable derivado del log, en contadores"
status: revised
revises: []
revised_by: [D162, D164]
---

# D45 — Progreso observable derivado del log, en contadores

*(Revisada por D162: la superficie viva ya no es un flag — es el default de
`yunta run`, sin **`--follow`**.)* *(Revisada por D164: los contadores se
derivan de la crónica, y cada superficie la dispone.)* Dos niveles (nodos del
DAG / tareas del ledger), `waiting` distinguido, contadores con contexto en
lugar de porcentajes (mienten con re-rutas, loops y promociones), cambios de
denominador siempre visibles y atribuibles a un evento, composición como árbol
jamás promediada. Superficies: `status`, la vista viva de `run`, tool MCP
`workflow_status`. Cero eventos nuevos: derivación pura.

Descartado: porcentaje único estimado, y cualquier progreso autorreportado por
agentes.
