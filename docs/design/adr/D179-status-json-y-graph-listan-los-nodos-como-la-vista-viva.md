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

1. **Una superficie que lista nodos lista el frame.** `status`, `--json` y
   `graph --run` leen el `RunFrame` del manifest congelado del run: todo
   `NodeFrame` del frame, en orden de declaración, los hijos un paso bajo
   su grupo, los que el modo excluye incluidos y etiquetados `skipped`,
   con las mismas palabras que la vista viva. `stats` conserva una fila
   por nodo que arrancó —una fila de stats es una medición—.
2. **Lo que un nodo espera vive en su estado.** `NodeState::Waiting { on:
   NodeWait }` distingue un gate —con su `external_ref`— de las preguntas
   que el nodo hizo y nadie respondió; `NodeDisplay::of(state)`, la única
   entrada, las dice: `waiting — asked 2 questions: q1, q2`, con la única
   oración, `text::asked_questions`, que también dicen la crónica y
   `PauseReason::Questions`.
3. **`--json` sube a `schema_version: 5` y `nodes` es una lista.** La
   presencia de un nodo cambia de significado —de «el run lo alcanzó» a
   «el frame lo declara»— y un mapa no puede decir orden ni grupo: `nodes`
   pasa a ser una lista en orden de declaración, cada entrada con `id`,
   `state`, `detail`, `group` y `waiting_on` tipado. La regla de
   `json::SCHEMA_VERSION` sube el número cuando un campo cambia de
   significado, y ese es el momento de darle la forma.
4. **Un `parallel` no anida otro.** `check` lo rechaza junto a sus tres
   hermanos `*InsideParallel`; un paso de sangría es exacto.

## Racional

Un lugar: qué nodos tiene un run y en qué orden se contesta una vez, y
toda superficie lo consume. Un reader que compare `status` con la región
viva ve la misma lista con las mismas palabras.

## Alternativas descartadas

Enhebrar `Option<&QuestionsAskedPayload>` por parámetro hasta cada
llamador de `NodeDisplay::of`, o cargarlo en el frame (`NodeFrame.asked`)
con una segunda entrada `NodeDisplay::framed`: el hecho nace en el estado
y ahí es donde falta; un frame con `asked` y un estado sin él es dos
lugares. Sangrar por la cadena de grupos: sostiene un anidado que nadie
corre. Que `status` imprima las filas de la región byte
a byte: la región es una vista en vivo con runner y tiempos, `status` una
página; comparten la lista y la palabra, no la fila.
