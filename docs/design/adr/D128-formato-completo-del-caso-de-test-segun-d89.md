---
number: D128
title: "Formato completo del caso de test según D89: `events:`, `never:` y `nodes: {id: {runs, reroutes}}`"
status: accepted
revises: []
revised_by: []
---

# D128 — Formato completo del caso de test según D89: `events:`, `never:` y `nodes: {id: {runs, reroutes}}`

Las tres aserciones que el Contrato §14 y D89 especifican se implementan con
la forma de la spec; la forma corta (`nodes: {id: estado}`) se conserva como
atajo equivalente a `{state}`.

Racional: la spec ya define el formato; la implementación parcial deja sin
poder afirmar secuencia, ausencia y conteo de re-rutas, que son exactamente lo
que distingue una verificación de una foto del estado final.

Descartado: redefinir el formato desde la forma corta (contradice la spec
publicada).
