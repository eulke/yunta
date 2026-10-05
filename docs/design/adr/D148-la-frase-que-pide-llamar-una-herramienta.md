---
number: D148
title: "La frase que pide llamar una herramienta la produce el montaje, no el bloque de contrato"
status: accepted
revises: []
revised_by: []
---

# D148 — La frase que pide llamar una herramienta la produce el montaje, no el bloque de contrato

`submission_notice` vive junto al listener y devuelve texto solo cuando hay
sesión montada y el nodo declara algo que chequear; el bloque de
`artifact-shape` se queda con las claves, los tipos y las reglas.

Racional: el contexto se ensambla antes de que se resuelva el runner, así que
el bloque no podía saber si la herramienta iba a existir — y prometía llamarla
igual, mientras `open_run_tools` devolvía su estado de reposo sin evento ni
diagnóstico. Decirle a un agente que llame una herramienta es una promesa, y
una promesa solo es sostenible donde la herramienta se montó: produciéndola
desde el montaje, «instruido pero no montado» deja de ser un caso a evitar y
pasa a ser irrepresentable.

Descartados: resolver el runner antes de ensamblar el contexto (reordena
eventos que hoy tienen un orden deliberado); chequear la capacidad desde el
bloque (dos módulos que tendrían que acordar, que es el defecto que esto
elimina).
