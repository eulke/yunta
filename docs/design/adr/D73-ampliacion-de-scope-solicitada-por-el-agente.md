---
number: D73
title: "Ampliación de scope solicitada por el agente y concedida por el engine o por una persona (Contrato §6.2)"
status: accepted
revises: []
revised_by: []
---

# D73 — Ampliación de scope solicitada por el agente y concedida por el engine o por una persona (Contrato §6.2)

Cubre el caso intermedio entre `finding_posted` (no me corresponde) y
`promotion_signaled` (excede el modo): el arreglo chico y adyacente que sale
más barato hacer ahora. El agente **nunca amplía**: emite una solicitud con
paths, razón, criterio verificable propuesto y consecuencia de la denegación —
**objeto idéntico en los tres modos**, para que la información no dependa del
destinatario y no existan dos calidades de decisión. Modos por nodo/workflow:
`rules` (engine evalúa `within` + tamaño + criterio en rojo), `ask` (gate §5.3
por consola, MCP o PR), `deny` (default; la solicitud se vuelve finding sin
interrumpir). El modo solo se endurece desde capas superiores, nunca se afloja
(§6.1). El engine aporta lo que el agente no sabe ni debe autoevaluar: conteo
de ampliaciones del run, colisión con otras tareas, tamaño del diff y
**resultado del criterio propuesto** — si ya pasa, es trivial y se rechaza sin
consultar. Cap `max_per_run` cuyo agotamiento pausa con escalación (varias
concesiones seguidas indican plan mal cortado). Todo queda en log
(`scope_expansion_requested|granted|denied`) y en el recibo; el diff final se
evalúa contra scope declarado más ampliaciones autorizadas.

Descartados: permitir que el agente amplíe y justifique después (drift con
extra pasos), y variar la información según decida una regla o una persona.
