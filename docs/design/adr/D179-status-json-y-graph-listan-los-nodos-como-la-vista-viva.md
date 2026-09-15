---
number: D179
title: "`status`, `--json` y `graph --run` listan los nodos como la vista viva: el frame del manifest congelado"
status: accepted
revises: []
revised_by: []
---

# D179 — `status`, `--json` y `graph --run` listan los nodos como la vista viva: el frame del manifest congelado

## Contexto

La vista viva lee el `RunFrame`: todos los nodos del modo, en orden de
declaración, cada grupo `parallel` seguido de sus hijos, del manifest
congelado del run. `status` y `--json` leen `RunState.nodes`: sólo los
nodos que el log nombra, en orden alfabético. `graph --run` enmarca el
workflow de disco, no el manifest del run, y sin los hijos de un
`parallel`. `NodeFrame.group` se escribe y nadie lo lee (M24 I-08). Un
nodo que preguntó dice `waiting` a secas porque `NodeDisplay` no llega a
`GateLedger::pending_questions` (plan de raíz, §11 L-67; `preguntas.md`
§5).

## Decisión

1. **Una superficie que lista nodos lista el frame.** `status`, `--json`,
   `graph --run` y `stats` leen el `RunFrame` del manifest congelado del
   run: todos los nodos del modo, en orden de declaración, los hijos bajo
   su grupo, con las mismas palabras que la vista viva. `stats` conserva
   una fila por nodo que arrancó —una fila de stats es una medición— y
   toma la palabra del frame.
2. **El frame lleva lo que la palabra necesita.** `NodeFrame.asked` lleva
   las preguntas que el nodo hizo y nadie respondió; `NodeDisplay::framed`
   las dice: `waiting — asked 2 questions: q1, q2`. `NodeDisplay::of(state)`
   se conserva para quien tiene un estado y no un frame: la crónica dice
   un momento, y un caso de `yunta test` juzga una palabra.
3. **`--json` sube a `schema_version: 5`.** La presencia de un nodo en
   `nodes` cambia de significado —de «el run lo alcanzó» a «el modo lo
   incluye», con `never ran` y `skipped` como etiquetas— y la regla de
   `json::SCHEMA_VERSION` sube el número cuando un campo cambia de
   significado.

## Racional

Un lugar: qué nodos tiene un run y en qué orden se contesta una vez, y
toda superficie lo consume. Un reader que compare `status` con la región
viva ve la misma lista con las mismas palabras.

## Alternativas descartadas

Enhebrar `Option<&QuestionsAskedPayload>` por parámetro hasta cada
llamador de `NodeDisplay::of`: el estado ya lo tiene, y cada sitio tendría
que acordarse de buscarlo. Que `status` imprima las filas de la región byte
a byte: la región es una vista en vivo con runner y tiempos, `status` una
página; comparten la lista y la palabra, no la fila.
