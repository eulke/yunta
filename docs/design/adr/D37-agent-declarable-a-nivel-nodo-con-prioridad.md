---
number: D37
title: "`agent:` declarable a nivel nodo, con prioridad sobre el del runner; el runner conserva el suyo"
status: accepted
revises: []
revised_by: []
---

# D37 — `agent:` declarable a nivel nodo, con prioridad sobre el del runner; el runner conserva el suyo

El override por nodo da flexibilidad (mismo runner, agente distinto por nodo)
sin definir N runners. El campo en el runner se mantiene porque es lo
que permite candidatos multi-adapter donde cada adapter empareja su propio
agente nombrado — quitarlo rompería la portabilidad de §13.1. La resolución
valida la existencia de todos los agentes (de candidatos y de nodos) por
`probe()`. Guía: runner-level en workflows compartidos, node-level en
internos.

Descartado: agente solo a nivel nodo.
