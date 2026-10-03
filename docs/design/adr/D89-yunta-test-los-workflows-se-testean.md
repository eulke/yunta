---
number: D89
title: "`yunta test`: los workflows se testean como el código que son (Contrato §14)"
status: accepted
revises: []
revised_by: []
---

# D89 — `yunta test`: los workflows se testean como el código que son (Contrato §14)

Casos declarativos en `.yunta/tests/` que combinan workflow + modo + inputs +
fixture del mock con un bloque `expect`, ejecutados sin LLM ni red y
comparados contra el estado derivado por replay. Aserciones sobre **estado
final y secuencia** (nodos, tareas, conteo de eventos, y `never` para lo que
no debe ocurrir): las regresiones de un workflow suelen ser de camino, no de
resultado, y sin aserciones de secuencia pasarían inadvertidas. Cero
maquinaria nueva: mock con fixtures (T3.2), derivación por replay (T2.3) y
estado del run ya existen — `test` es un comparador. Los packs pueden traer
sus tests: `pack add` no los corre por default, pero `pack audit` reporta si
existen y si pasan, lo que convierte la calidad de un pack en dato verificable
en lugar de promesa del autor (insumo natural del marketplace, RFC-0003 §3).
Racional de fondo: un producto cuyo argumento es la verificación mecánica no
puede dejar sin verificar sus propios artefactos — "correr end-to-end con
mock" no es testear.
