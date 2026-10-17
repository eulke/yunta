---
number: D125
title: "Dependencias sobre nodos excluidos por modo: reescritura transitiva"
status: accepted
revises: []
revised_by: []
---

# D125 — Dependencias sobre nodos excluidos por modo: reescritura transitiva

Cuando un modo excluye un nodo, cada nodo que dependía de él hereda las
dependencias incluidas del excluido, transitivamente; una única función deriva
el grafo de cada modo para el scheduler, la escalación y `status`, y el engine
tiene un test que afirma el orden de ejecución.

Racional: una arista hacia un nodo excluido que se da por satisfecha reordena
el run en silencio — en un modo `quick`, un gate y el nodo que publica el PR
corrían antes que la deliberación que los precede — y ningún test lo detectaba
porque ninguno afirmaba orden. La reescritura conserva la intención del autor
(los modos recortan deliberación, D21) sin obligarlo a duplicar `depends_on`
por modo.

Descartado: error de `check` al depender de un nodo excluido (fuerza a
declarar el grafo una vez por modo); dar por satisfecha la arista (el
comportamiento que reordenaba en silencio).
