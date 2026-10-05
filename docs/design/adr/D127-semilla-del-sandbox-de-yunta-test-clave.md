---
number: D127
title: "Semilla del sandbox de `yunta test`: clave `worktree:` en el caso"
status: accepted
revises: []
revised_by: []
---

# D127 — Semilla del sandbox de `yunta test`: clave `worktree:` en el caso

Un caso de test puede declarar `worktree: <directorio>`, relativo al caso,
cuyo contenido se copia al sandbox antes del commit inicial; sin la clave, el
sandbox arranca vacío.

Racional: cada caso corre en un repositorio recién creado, y un workflow cuyos
nodos leen archivos (`files:`), reciben un input `path` o corren la toolchain
del repo no se puede probar sin que algo escriba esos archivos por un camino
ajeno a su rol. La semilla es declarativa, se ve en el caso y no cambia el
fixture del mock.

Descartado: que el fixture del mock escriba archivos fuera de su rol; un
repositorio de fixture completo por caso.
