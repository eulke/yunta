---
number: D184
title: "Declarar `scope:` es pedir un árbol propio"
status: accepted
revises: []
revised_by: []
---

# D184 — Declarar `scope:` es pedir un árbol propio

## Contexto

Dos nodos que corren a la vez comparten el checkout del run, así que el diff de
cada uno arrastra lo que escribió el otro. Con el binario real, dos hijos de un
`parallel` con scope disjunto declarado, cada uno escribiendo sólo dentro del
suyo, fallan los dos: `sweep-a violations: ['b/out.txt']`, `sweep-b violations:
['a/out.txt']`. Lo mismo, sin concurrencia ninguna, entre un nodo y su
predecesor, porque nada commitea entre nodos de nivel superior.

D182 fija que una unidad de trabajo se audita contra el árbol del que partió, y
que donde dos unidades corren a la vez cada una tiene el suyo — porque si no,
«el árbol del que partió» es una frase sin referente. Queda por decidir quién es
una unidad: a quién le da el engine un checkout aparte.

La respuesta no es libre. Un checkout aparte es un `git worktree add` limpio y
aterriza con `add -A`, que respeta `.gitignore`, así que lo que un nodo produzca
bajo un path ignorado no existe en su árbol al abrir ni aterriza al cerrar. Hoy
sí cruza: con un `.gitignore` que lleva `build/`, un hijo de `parallel` que
escribe `build/out.bin` y un nodo posterior que lo lee, el run termina 4/4.
Aislar a alguien le quita eso.

## Decisión

**Recibe un árbol propio el nodo que declara `scope:`, y nadie más.** En una
frase: declarar `scope:` es pedir un árbol propio.

1. **El conjunto aislado es el que `audited_scope` ya define**: un nodo con
   `scope:` no vacío, y un nodo `permissions: read-only`, que se audita contra el
   scope vacío. Un nodo que no declara nada conserva el árbol del run.

2. **No depende de cómo corre.** Ni de `kind: parallel`, ni de que el scheduler
   lo haya puesto en el mismo batch que otro bajo `max_parallel_nodes`. Depende
   de lo que el autor escribió en su workflow, que es lo único que ese autor
   puede ver.

3. **Una unidad aterriza al cerrar**, con el mecanismo que D182 ya fijó: replay
   sobre el árbol compartido como está en ese momento, verificación ahí, y
   fast-forward. Un replay que git no puede terminar falla el nodo nombrando los
   paths, porque los checks estáticos ya probaron que no debía haberlos.

4. **Las tareas de un `kind: loop` siguen aisladas siempre**, declaren scope o
   no: una tarea recibe un árbol porque es una unidad por construcción, y ese
   mecanismo es el que este generaliza, no uno que revise.

## Racional

La regla la enuncia el autor, no el motor. «Aislar todo lo concurrente» se dice
como «depende de si el motor te puso en el mismo batch que otro», y «aislar los
hijos de `parallel`» como «depende de tu `kind`»: las dos describen el motor, y
un usuario no puede predecirlas sin saber cómo corre el scheduler. Ésta se dice
con una declaración que el autor escribió y puede leer.

Cierra el hallazgo entero y no una mitad. Un nodo aislado no ve escribir a nadie
—tenga scope el hermano o no, sea hijo de `parallel` o vecino de batch— y un
nodo sin `scope:` no tiene auditoría que pueda equivocarse, así que no necesita
árbol. No queda un tercer caso.

Los checks estáticos pasan a cobrar exactamente donde hablan.
`CheckError::OverlappingScope` y `CheckError::OverlappingFanOutScope` son reglas
*sobre scopes declarados*: exigen disjunción entre escritores concurrentes que
declararon algo. El runtime aísla a ese mismo conjunto, con lo que la disjunción
que `check` ya probó es lo que hace que un aterrizaje sea sin conflicto por
construcción. Estático y runtime dicen lo mismo, sobre los mismos nodos.

Y una advertencia deja de mentir. `CheckWarning::UndeclaredParallelScope` sugiere
declarar scope para que el check sea real, y hoy declararlo es exactamente lo que
lo rompe: te hace responder por lo que escribió el hermano. Bajo esta decisión el
texto se vuelve literal, sin reescribirlo.

El costo lo paga quien lo pidió. Un nodo con `scope:` deja de ver lo que un
hermano produzca bajo un path ignorado por git; un workflow que compile en un
nodo con scope y consuma `target/` en otro cambia de comportamiento. Es el nodo
que declaró un límite el que recibe un límite, y es el único que cambia: de cinco
nodos de un workflow de ejemplo —dos en un batch de fan-out, tres hijos de un
`parallel`, dos de ellos con scope— cambian dos.

Queda en pie, a propósito, que dos nodos sin scope que se pisan se siguen
pisando: el último gana, con la advertencia de `check` como único aviso. Es el
comportamiento de hoy, nadie lo reportó, y romperlo sería cobrarle un límite a
quien nunca lo pidió.

Esta decisión se toma con la herramienta en manos de tres personas que la están
probando, donde un cambio de comportamiento cuesta un mensaje y no una
migración, y deja esa condición escrita.

## Alternativas descartadas

**Aislar todo lo concurrente.** Es lo que M32 había firmado. Le quita a todo nodo
concurrente lo ignorado por git, y hace que la visibilidad de archivos dependa de
`max_parallel_nodes`: el mismo workflow comparte árbol en 1 y no en 2. Una
perilla de rendimiento pasaría a cambiar lo que un nodo ve, que es la clase de
regla que no se puede enunciar sin describir el scheduler.

**Aislar sólo los hijos de `kind: parallel`.** Aísla la concurrencia que el autor
declara y no la que sale de una perilla, lo que es la mitad buena del argumento.
Deja abierta la otra mitad del hallazgo: bajo `max_parallel_nodes > 1`, dos nodos
independientes con scope disjunto se siguen culpando mutuamente, que es el caso
que `OverlappingFanOutScope` existe para prevenir y que seguiría sin cobrarse.

**Copiar lo ignorado por git al abrir la unidad y al aterrizar.** Conservaría el
cruce de `target/` y compañía. Es un segundo mecanismo al lado del que ya
funciona, y «cuáles archivos ignorados» no tiene respuesta de principios: copiar
todos hace de cada apertura una copia del árbol entero, y elegir algunos es un
umbral que nadie puede fijar.
