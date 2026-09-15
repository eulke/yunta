---
number: D64
title: "La preparación del árbol es el costo dominante de wall-clock; se ataca con cachés compartidas, no salteando aislamiento"
status: accepted
revises: []
revised_by: []
---

# D64 — La preparación del árbol es el costo dominante de wall-clock; se ataca con cachés compartidas, no salteando aislamiento

Directorio de artefactos común entre worktrees, dependencias enlazadas, o
worktrees reutilizables por proyecto. `yunta init` detecta el ecosistema y
propone la configuración; la guía de autoría lo documenta como patrón de
primera clase, junto con la granularidad de criterios (criterio acotado por
tarea, `guard` global una sola vez al cierre — en lugar de suite completa por
tarea).

Descartado: que el engine infiera qué tests corresponden a qué scope (frágil y
específico por ecosistema; es autoría, no magia).
