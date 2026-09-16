---
number: D49
title: "Coordinación entre nodos paralelos: `independent` por default, `blackboard` opt-in por grupo"
status: accepted
revises: []
revised_by: []
---

# D49 — Coordinación entre nodos paralelos: `independent` por default, `blackboard` opt-in por grupo

Atributo `coordination:` del nodo `parallel`. Con `independent` (default), los
nodos del grupo no se ven: las tools de blackboard ni se montan — el patrón
correcto para grupos evaluativos (reviewers), donde la contaminación cruzada
produce anclaje y mata la diversidad de hallazgos que el fan-out compra; la
deduplicación es post-hoc en un nodo consolidador. Con `blackboard`, los nodos
reciben el endpoint MCP por-run con
`yunta_post_finding`/`yunta_get_blackboard` **scopeadas al grupo** (jamás al
run entero): para grupos cooperativos (executors paralelos que descubren
problemas compartidos, consolidador que arranca temprano). Todo posteo queda
como `finding_posted` mediado por el engine — D26 intacto.

Descartado: blackboard como default (ancla juicios), visibilidad cross-grupo
(filtración entre paralelos no relacionados), y cualquier canal no mediado.
