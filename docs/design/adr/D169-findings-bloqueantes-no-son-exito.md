---
number: D169
title: "Un run que cierra con findings bloqueantes reporta `Reported`, no `Success`"
status: accepted
revises: []
revised_by: []
---

# D169 — Un run que cierra con findings bloqueantes reporta `Reported`, no `Success`

`Outcome` deriva de `RunWord` en un solo lugar, y un run cuyo estado es
`Finished` con `blocking_findings > 0` sale con código 1 (`Reported`). El
documento JSON lleva el conteo.

Racional: el propio bloque de cierre dice "finished, holding N blocking
findings — nobody has accepted this work"; un código 0 contradice la línea
que lo acompaña, y hoy el mapeo vive en dos lugares (`drive::verdict` y
`Closing::outcome`) que solo coinciden por atención.

Descartados: 0 con el conteo en el JSON (es lo de hoy, decidido por
herencia, no por decisión).
