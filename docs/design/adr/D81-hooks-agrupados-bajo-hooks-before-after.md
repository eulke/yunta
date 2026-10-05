---
number: D81
title: "Hooks agrupados bajo `hooks: {before, after}` (Contrato §11.1)"
status: accepted
revises: []
revised_by: []
---

# D81 — Hooks agrupados bajo `hooks: {before, after}` (Contrato §11.1)

`before` y `after` dejan de ser claves sueltas del nodo: solo tienen sentido
juntos, comparten reglas (determinismo, idempotencia, timeout, evento
`hook_executed`, sujeción al scope) y agruparlos reduce la superficie plana
del nodo. A nivel workflow, `node_defaults.hooks`. Criterio de agrupamiento
fijado para futuras claves: **se agrupa cuando las claves solo tienen sentido
juntas y comparten reglas, no cuando meramente pertenecen al mismo tema.** Por
eso se descartó agrupar `scope`, `criteria`, `artifacts` y `on_failure` bajo
una property de verificación: se usan de forma independiente, son el corazón
del contrato, y anidarlas las haría menos visibles justo donde la visibilidad
es el punto — además de obligar a recordar en qué grupo vive cada cosa, que es
el problema opuesto al que se quería resolver.
