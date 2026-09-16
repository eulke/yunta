---
number: D159
title: "Un run se hace cargo de las tareas de todo documento de tareas que adquiere; de otro run cruza lo hecho cuyo trabajo el árbol receptor ya tiene"
status: accepted
revises: [D157]
revised_by: []
---

# D159 — Un run se hace cargo de las tareas de todo documento de tareas que adquiere; de otro run cruza lo hecho cuyo trabajo el árbol receptor ya tiene

Cada vez que un documento de tareas entra a un run, el run afirma qué tiene
que hacer con él: un `task_registered` por tarea, en orden del documento, bajo
el nodo por el que entró —o sin nodo, al nacer—. Son cinco puertas y una sola
rutina, `tasks::register`: el nacimiento desde un input, el nacimiento por
promoción, el nacimiento por mount, el cierre del nodo que produce o entrega
el documento, y el cierre del nodo `kind: workflow` que lo adquiere de su
hijo. Dos modificadores independientes deciden si a un registro le sigue un
`task_status_changed`. **Reset por identidad** (este log): si este log ya
registró el id con otros `criteria` u otro `scope`, la tarea vuelve a
`pending` con el registro como `caused_by` —§5.7 tal como ya existía—.
**`done` que cruza** (el log fuente y el árbol receptor): si el documento
viene de otro run (`origin: inherited`), ese log deja la tarea `done` y el
commit donde su trabajo aterrizó es ancestro del HEAD del árbol en el que este
run va a trabajar, nace `done` acá, con ese mismo commit. El reset gana: un
`done` ajeno sobre una tarea que este run cortó distinto no dice nada sobre la
tarea nueva, por la misma razón por la que §5.7 existe. Y un `done` que cruza
sobre una tarea que este log ya tiene `done` no emite nada, porque el replay
de `task_registered` es `or_insert`. Para que la ascendencia sea preguntable,
`task_status_changed` gana un campo opcional `commit`, que un `done` lleva y
ningún otro estado: lo emite la integración del `loop`, con el commit en el
que quedó el árbol del run al hacer el `merge --ff-only` de la tarea —una
tarea que no commiteó nada aterriza en el head de integración, y registrarlo
es exacto: está vacuamente en todo árbol que descienda de ahí—. El que cruza
lo lleva también, con lo que el tercer eslabón de una cadena de promociones
contesta la misma pregunta sobre su propio árbol. `CreateRunParams` gana el
`worktree` del run, porque el nacimiento tiene que poder preguntarle a ese
árbol qué ya tiene. `done` sigue siendo el único estado que viaja: `failed` y
`blocked` son lo que pedía una decisión que la promoción o la composición ya
tomó, `running` es una huérfana que se re-corre, y `ready` no se emite. El
nacimiento resuelve todo esto antes de crear el directorio del run: lee cada
documento y, por cada run fuente distinto, el estado que su log deja en pie;
un fuente que no replaya o que no se puede leer devuelve error ahí, y el run
no llega a tener directorio ni `run_created`, mismo racional que un input
`document` inválido (§2.3). El origen de un artifact de nacimiento gana su
propio tipo, `BirthOrigin`, con las dos formas que existen antes de que corra
un nodo —`input` e `inherited`—, porque `submitted`, `ingested`, `derived` y
`answered` nombran cosas que un run recién nacido no pudo hacer. Y el `loop`
verifica una vez, antes del primer lote, que toda tarea del documento que el
run tiene esté registrada: la que no lo está marca el run `broken` nombrando
el documento y los ids, en vez de un lote vacío que reporta que nada está
listo.

Racional: el sucesor de una promoción nacía con el documento de tareas del
antecesor y sin una sola tarea registrada, con lo que su `loop` no formaba
lote, no daba todo por hecho y fallaba con «no task is ready and not all are
done» —un workflow con `inputs: {tasks: {type: document}}` que promovía a un
modo sin planificador no podía terminar—; un hijo que recibía su documento por
mount fallaba igual; y un padre que adquiría el documento de su hijo lo
registraba entero `pending` aunque el log del hijo dijera `done`. Si el
sucesor re-planificaba, el diff de §5.7 corría contra un mapa vacío y toda
tarea nacía `pending`, incluidas las `done` cuyo commit ya estaba en el árbol:
el run afirmaba algo falso sobre su propio worktree. Que decida la ascendencia
y no la puerta sale de que el log no registraba dónde aterrizaba el trabajo de
una tarea, y sin ese hecho la regla no era verificable: había que apoyarse en
un racional sobre la puerta —«el árbol del receptor nace del árbol donde eso
está integrado»—, que es verdadero para la promoción y para `isolation:
inherit`, y falso para las dos puertas de composición restantes. Un hijo
`kind: workflow` con el aislamiento por default recibe una rama fresca
**desde** el árbol del padre y no existe merge-back en ninguna parte, así que
su trabajo se queda en su rama: un padre que adquiere su documento marcaría
`done` trabajo que su árbol no tiene, y un hermano que monta ese documento y
corre un `loop` nacería con todo `done` sobre un árbol vacío, salteando el
trabajo en silencio. Con el commit en el evento, la ascendencia es el hecho
que la regla siempre quiso afirmar, y cada caso sale solo en vez de cablearse.
Los payloads se reemplazan en el lugar, sin `task_status_changed_v2`, que es
lo que D141 autoriza mientras no haya tag publicado; el campo es aditivo y
opcional, así que un log anterior sigue leyéndose.

Descartados: registrar todo lo heredado como `pending` sin más (resuelve el
fallo y rehace trabajo integrado, y el sucesor nacería afirmando algo falso
sobre su propio árbol); cargar `failed` y `blocked` tal cual (el `loop` del
sucesor fallaría en su primera iteración por el motivo que promovió); acotar
el cruce a la promoción y a `isolation: inherit` por origen (depende de qué
puerta y no de qué es cierto, y se rompe con la próxima forma de herencia);
usar el head final del run fuente en vez del commit de cada tarea (menos
preciso —una tarea integrada temprano queda indistinguible de una que el run
nunca terminó de arrastrar— y obliga a un hecho nuevo de run en vez de uno de
tarea, que es de lo que la pregunta trata); probar la rama del run fuente por
nombre (`Isolation::None` no tiene rama propia, y una rama se borra); dejar
cruzar un `done` sin commit (no distingue «no hubo trabajo» de «el log no lo
dice», y la segunda lectura hace perder trabajo); que el antecesor materialice
el estado en el documento al cerrar (el documento es una foto congelada y el
progreso son eventos —D11—, así que un documento con estado es el archivo
mutable que D11 descartó); copiar los eventos de tareas del antecesor al log
del sucesor (seqs ajenos y dos runs en un log, que rompe I3); exigir por
`check` que todo modo con `loop` tenga un productor de tareas (restricción en
lugar de mecanismo: obliga a re-planificar en cada promoción y pierde el
`done` igual); que cada llamador —promoción, mount— calcule lo que cruza y lo
pase en el `BirthArtifact` (tres sitios que pueden olvidarse, cuando el
nacimiento tiene el `storage`, el árbol y el origen que nombra el run); un
campo `carried` en `BirthArtifact` (un campo sin sentido para todo artifact
que no sea un documento de tareas); dejar la adquisición desde un hijo afuera
(mismo origen y otra regla: es la copia que señala el lugar que falta); un
evento propio para lo que cruza, `task_inherited` (lo que ya afirman dos
eventos existentes, con peor compatibilidad); crear el run y fallar después si
el fuente no replaya (deja un run con `run_created` y sin lo que declaró; el
rechazo va antes de que exista).
