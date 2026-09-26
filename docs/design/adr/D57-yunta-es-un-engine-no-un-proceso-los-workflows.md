---
number: D57
title: "Yunta es un engine, no un proceso; los workflows de fábrica se distribuyen como packs"
status: accepted
revises: []
revised_by: []
---

# D57 — Yunta es un engine, no un proceso; los workflows de fábrica se distribuyen como packs

El engine no propone flujo alguno: impone una *forma* (invariant-nodes,
criterios ejecutables, scope, pre-check en rojo — la definición operativa de
"trabajo verificado"), nunca un proceso. Nada de workflows embebidos en el
binario: `yunta/starter` (mínimo, enseña la forma y sirve de fixture) y
`yunta/fragua` (referencia completa extremo a extremo) son packs instalables
sin más estatus que los de un tercero, borrables sin afectar capacidades del
engine. El README enseña primero a escribir un workflow de tres nodos desde
cero y recién después menciona packs.

Racional: si los flujos de fábrica vienen embebidos y muy pulidos, "Yunta"
pasa a significar ese flujo — se pierde la genericidad que es el producto.

Descartado: workflows default compilados en el binario.
