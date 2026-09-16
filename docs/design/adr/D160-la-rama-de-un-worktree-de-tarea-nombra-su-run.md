---
number: D160
title: "La rama de un worktree de tarea nombra su run, y las dos familias de rama son hermanas bajo prefijos fijos"
status: accepted
revises: []
revised_by: []
---

# D160 — La rama de un worktree de tarea nombra su run, y las dos familias de rama son hermanas bajo prefijos fijos

El engine crea ramas en dos lugares: la del run (`run_branch`, lo que
`{{run.branch}}` rinde y lo que el cierre limpia) y la de cada intento de cada
tarea (`task_branch`, la rama del worktree que el loop le da a una tarea).
Quedan `yunta/run/<run_id>` y `yunta/task/<run_id>/<task_id>/<intento>`,
compuestas las dos en `worktree`, de donde las leen el dispatch, la promoción,
la composición, el cierre y los diagnósticos. Dos propiedades, y cada una
responde a una falla concreta. **La rama de una tarea nombra su run** porque
el worktree es del run pero un ref es de todo el repositorio: dos runs sobre
un mismo checkout llegan al mismo `task_id` —el mismo plan corrido dos veces,
un sucesor rehaciendo lo que su antecesor dejó abierto— y un nombre construido
solo con la tarea le pide a git la misma rama dos veces, que el segundo run no
puede tener (`fatal: a branch named 'yunta/task/T001/1' already exists`).
**Las dos familias divergen en el segmento siguiente a `yunta/`**, los dos
literales fijos, porque un ref no puede ser directorio de otro: `refs/heads/a`
y `refs/heads/a/b` no coexisten, en ningún orden.

Racional: el número de intento se cuenta sobre el log de un run solo, así que
dos runs arrancan los dos en 1 y el nombre viejo colisionaba en cuanto dos
runs compartían checkout; el aislamiento que la ruta del worktree sí tenía
(`run.dir/task-worktrees/`) el ref no lo tenía.

Descartados: anidar la tarea bajo la rama del run,
`yunta/<run_id>/task/<task_id>/<intento>` (es exactamente el conflicto
directorio/archivo: crear esa rama vuelve imposible la del run, y crear la del
run vuelve imposible la de la tarea — verificado contra git en los dos
órdenes, y es la razón por la que los prefijos son hermanos y no anidados);
dejar la tarea bajo `yunta/task/<run_id>/...` sin mover la rama del run de
`yunta/<run_id>` (funciona salvo para un run llamado literalmente `task`, que
`RunId` admite porque solo exige un segmento de path: una colisión angosta
sigue siendo una colisión, y elegir una forma que la conserva repite en chico
el defecto que esta decisión existe para sacar); borrar la rama de la tarea al
integrarla (quita la colisión y pierde el rastro forense de una tarea fallida,
y `branch -d` se niega con una rama sin mergear, así que una integración
rechazada filtra el nombre igual); contar el intento sobre todo el repositorio
en vez de sobre el log del run (hace que el nombre de una rama dependa de la
historia de otros runs, que es más acoplamiento y no menos).
