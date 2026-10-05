---
number: D182
title: "Una unidad de trabajo se audita contra el árbol del que partió, y ese árbol es un hecho del log"
status: accepted
revises: []
revised_by: []
---

# D182 — Una unidad de trabajo se audita contra el árbol del que partió, y ese árbol es un hecho del log

## Contexto

`scope:` es, por diseño, un borde post-hoc: ningún adapter bloquea una escritura
fuera de scope mientras ocurre, así que el engine compara después lo que cambió
contra lo que el nodo declaró (`docs/guide.md:75-83`). La comparación se hacía
contra `HEAD` —el commit base del run— y nada commitea entre nodos de nivel
superior, de modo que cada nodo era juzgado por el diff acumulado de todo el run.

Medido con el binario real: un nodo con `scope: ["bar/**"]` que escribe sólo
dentro de su scope falla por un archivo que escribió un nodo anterior
(`violations: ['loose.txt']`). Bajo concurrencia, dos hijos de un `parallel` con
scope disjunto declarado se culpan mutuamente. Las tareas de `kind: loop` no lo
sufren, y la razón es la respuesta: cada intento corre en su propio árbol y
aterriza antes del siguiente.

Es decir: el engine respondía «¿qué cambió esta unidad de trabajo?» por dos
caminos —el árbol propio de una tarea, el estado ambiente del disco para un
nodo— y sólo uno era correcto.

## Decisión

1. **El punto de partida es un hecho del log.** `node_started` lleva el árbol del
   que ese intento parte (`from_tree`), y la auditoría es la diferencia contra
   él. Deja de ser una referencia ambiente que nadie registró: un veredicto pasa
   a derivarse de dos hechos persistidos, así que sobrevive al replay y a un
   reinicio de intento. Un log escrito antes de este campo lo lee ausente y
   deriva contra la base del run, que es lo que ese log significaba.

2. **Un árbol, no una lista de paths.** El punto de partida es un objeto `tree`
   de git, no el conjunto de archivos que estaban sucios al arrancar. La
   diferencia importa: con una lista, un archivo que un nodo anterior dejó sucio
   y que *este* nodo también modifica quedaría excluido, y una escritura fuera de
   scope pasaría sin verse. Con un árbol, el contenido difiere y la escritura se
   ve.

3. **La cáscara y el núcleo, separados.** `capture_tree` y `changed_since` hablan
   con git; `violations` es una función pura del diff, el scope y lo que el
   adapter declaró suyo. `scope_check`, que mezclaba las dos cosas y traía su
   punto de partida como supuesto, se borra.

4. **Concurrencia implica aislamiento.** Donde dos unidades de un run pueden
   correr a la vez —hijos de un `parallel`, nodos independientes bajo
   `max_parallel_nodes > 1`— cada una abre su propio árbol y aterriza, como una
   tarea ya hace. Sin eso, «el árbol del que partió» es una frase sin referente:
   un hermano escribe después de la captura y aparece en un diff que no es suyo.
   El aterrizaje sale de `loop_exec` y se generaliza; una tarea deja de ser el
   caso especial y pasa a ser la primera instancia de la regla.

5. **La disjunción que `check` exige es la que hace el aterrizaje sin
   conflicto.** `RuleCode::OverlappingScope` y `CheckError::OverlappingFanOutScope`
   ya rechazan dos escritores concurrentes con scopes que se tocan. Esa garantía
   estaba verificada y sin consumidor; ahora es lo que sostiene el rebase. Un
   conflicto al aterrizar deja de ser un caso a resolver y pasa a ser un workflow
   que no debió pasar `check`.

## Racional

Un lugar: la pregunta «¿qué cambió esta unidad?» tiene un solo mecanismo que la
responde, y es el que ya existía y funcionaba. Replay: el punto de partida es un
hecho y no memoria de una invocación. Núcleo puro: el juicio se separa de la
llamada a git. Degradación explícita: un chequeo que no puede ser cierto deja de
prometer que lo es. Y sin basura: lo que se generaliza —el aterrizaje de una
tarea— nunca se reimplementa.

## Alternativas descartadas

**Un punto de partida en memoria, capturado al arrancar el nodo.** No sobrevive
al replay: si el engine muere entre el arranque y el cierre, el reinicio captura
de nuevo y el trabajo parcial del intento anterior queda del lado de «ya estaba
sucio», con lo que una escritura fuera de scope se pierde. Y no cubre el
fan-out, donde los concurrentes no son un grupo declarado sino un conjunto que
sólo existe en runtime: excluir «el scope de los hermanos» pediría un registro
vivo de quién corre, que es un mecanismo nuevo para compensar la ausencia del
que ya existe. Además se debilita cuanto más concurrencia hay, que es
exactamente cuando el chequeo importa más.

**Un commit por nodo al cerrar.** Arregla el caso secuencial y no el concurrente:
los hermanos siguen partiendo del mismo `HEAD` y viéndose entre ellos. Y cambia
la forma visible de la rama del run en toda corrida, a cambio de la mitad del
problema.

**Dejarlo y corregir sólo la promesa** —que la documentación y el aviso de
`check` digan que `scope:` se audita contra el diff acumulado del run—. Es
honesto y cuesta poco, y sería la salida correcta si el mecanismo no valiera una
fase. Se descarta porque el mecanismo ya está construido para las tareas: lo que
falta no es inventar nada, es dejar de tener dos.
