---
number: D158
title: "Reanudar verifica del worktree su identidad y su ascendencia; su contenido no se verifica porque el contenido es el trabajo"
status: accepted
revises: []
revised_by: []
---

# D158 — Reanudar verifica del worktree su identidad y su ascendencia; su contenido no se verifica porque el contenido es el trabajo

Antes del `run_resumed`, y al lado de la verificación por hash de los
artifacts (D157), el resume le pregunta al worktree dos cosas. **Identidad**:
hay un working tree de git en la ruta que el manifest congeló —todo lo que
sigue (el diff de scope, el `tree_hash` de los criterios, el checkout de cada
tarea) corre git ahí adentro, así que un directorio que no lo es convierte
cada paso posterior en la misma falla con peor mensaje. **Ascendencia**: el
`base_commit` del manifest sigue alcanzable desde el HEAD de ese árbol, un
`git merge-base --is-ancestor`. Un árbol que perdió ese commit marca el run
`broken` por el camino de `steps::broken`, con su export forense y sin
registrar que reanudó, y con un diagnóstico que nombra el worktree, el
`base_commit` y el HEAD que encontró. Que no haya working tree en esa ruta es
en cambio un error tipado con remedio —`WorktreeError::RunWorktreeLost` bajo
`isolation: worktree`, `NotACheckout` bajo `none`— que nombra la rama del run
y el `git worktree add` que la trae de vuelta. Las dos verificaciones corren
igual en las dos isolations; lo único que cambia por isolation es el remedio,
porque bajo `none` el run trabaja sobre el checkout del usuario y no tiene
rama propia a la que mandarlo. La verificación vive en `worktree::integrity`,
con la forma de `artifacts::integrity`, y `head_commit` y `run_branch` pasan a
ser del módulo `worktree`, de donde los leen el batch que ramifica, la
promoción que construye encima, el cierre que limpia, `{{run.branch}}` y este
diagnóstico.

Racional: el contrato (§8.1) prometía que el resume verifica la integridad de
run.dir **y del worktree**, y D157 cumplió la mitad de run.dir; la otra mitad
no podía cumplirse copiando la primera, porque las dos cosas no tienen la
misma naturaleza. Un artifact es inmutable: preguntarle si sigue siendo sus
bytes tiene una sola lectura, y una diferencia es corrupción. Un worktree
**es** el trabajo y cambia por diseño: entre una pausa y un resume una persona
lo abre, arregla algo a mano, corre los tests y commitea, y en un gate eso es
exactamente lo que el run está esperando —una regla «el worktree debe hashear
igual que al pausar» reportaría como corrupción el uso normal del sistema.
Frente a algo mutable el sistema ya responde bien y no hace falta inventar
nada: el memo de criterios se indexa por `tree_hash` (HEAD + diff completo +
el hash de cada archivo untracked), así que un árbol que se movió vuelve a
correr sus criterios en vez de reusar un resultado sobre un árbol que ya no
está. Re-verificar es la respuesta a lo mutable; rechazar es la respuesta a lo
inmutable. Lo que sí es unívoco es la ascendencia: toda tarea `done`, todo
`scope_checked` y todo criterio verde del log se establecieron contra un árbol
que descendía del `base_commit`, así que un HEAD que dejó de descender de él
deja al estado derivado describiendo un árbol que no existe, y ninguna
re-verificación los reconcilia. Es además barato: un proceso de git que no lee
el árbol. Consecuencias: un `reset --hard` por detrás de los commits del run,
un rebase o el checkout de otra historia son `broken` con diagnóstico en vez
de un run que sigue trabajando sobre terreno que su log nunca vio; un worktree
borrado para liberar espacio es recuperable sin perder el run; y
`docs/troubleshooting.md` gana la entrada del caso.

Descartados: verificar el worktree por hash de contenido como se verifica un
artifact (falla el uso normal del sistema, y una garantía sobre algo mutable
no es más fuerte sino falsa); exigir el árbol limpio al reanudar (es la misma
regla en versión débil, y prohíbe justamente lo que un gate pide que la
persona haga); saltear o degradar la ascendencia bajo `isolation: none` porque
el checkout es del usuario (el estado derivado describe esa historia igual que
cualquier otra, y dos lecturas de la misma verificación decididas por la
isolation es exactamente la clase de excepción que después nadie sabe leer; lo
que la isolation cambia es el remedio); marcar `broken` un worktree ausente
(la evidencia del run —log y objetos— está intacta y git conserva la rama, así
que el estado se recupera con un comando: `broken` es para un run que no puede
responder por su propia historia, y este puede); recrear el worktree ausente
por su cuenta (lo recrearía en `base_commit`, perdiendo todo commit del run, y
un resume que altera el árbol sin que nadie lo pida es lo contrario de
verificar); verificar además que sea un worktree *enlazado* y no un checkout
principal (la ruta sale del manifest congelado y no de nadie que la escriba,
así que es rigor sobre algo que nadie puede equivocar); llevar la verificación
del worktree también a `yunta verify` (responde por el terreno sobre el que el
run está por trabajar, no por su evidencia, y `verify` se corre sobre runs
cuyo worktree ya no tiene por qué existir —`gc` borra el run.dir y el log
sobrevive—); un evento propio para el resultado (misma lectura que D157: el
`broken` ya lleva el diagnóstico entero); un hash del árbol registrado al
pausar para poder contar qué cambió (dato que nadie decide con, y que le daría
al worktree una contabilidad paralela que el propio `tree_hash` ya deriva
cuando hace falta).
