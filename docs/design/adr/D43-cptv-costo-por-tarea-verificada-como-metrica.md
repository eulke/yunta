---
number: D43
title: "CPTV (costo por tarea verificada) como métrica de cabecera"
status: accepted
revises: []
revised_by: []
---

# D43 — CPTV (costo por tarea verificada) como métrica de cabecera

Usage atribuido a nodo/tarea; el engine deriva del log CPTV, tasa de
re-trabajo, tasa de cache y costo por nodo/rol/modo. `yunta stats` por run y
agregado histórico por workflow para decidir modos/runners/versiones con
datos.

Racional: minimizar tokens optimiza lo equivocado; el objetivo es el costo por
unidad de trabajo demostrada.

Descartado: telemetría de costos como feature externa — es derivable del event
log que ya existe.
