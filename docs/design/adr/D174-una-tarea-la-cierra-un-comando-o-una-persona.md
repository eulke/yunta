---
number: D174
title: "Una tarea la cierra un comando o una persona: `manual_review` y `justification` se retiran"
status: accepted
revises: [D14]
revised_by: []
---

# D174 — Una tarea la cierra un comando o una persona: `manual_review` y `justification` se retiran

## Contexto

D14 reserva un único juicio de un LLM sobre completitud: una tarea marcada
`manual_review: true`, con una `justification` de por qué ningún comando la
cierra, que un nodo de auditoría con rúbrica fija juzga. El documento de
tareas publica los dos campos a quien lo escribe (`tasks/shape.yaml`: "An
audit node judges whether this task is complete"; "say why a command cannot
settle it") y una regla rechaza `manual_review: true` sin `justification`.

El nodo de auditoría no existe. Una tarea con `manual_review: true` cierra
mecánicamente por sus `criteria` como cualquier otra, y la justificación que
el agente está obligado a escribir no la lee nadie: la regla asegura la
coherencia del par, no el juicio que el par anuncia. Son dos campos que un
agente tiene que acertar en un documento cuya forma ya le costó acertar, para
nada.

## Decisión

Una tarea la cierra un comando o una persona. Los `criteria` son la única
verificación de una tarea; lo que un comando no puede cerrar no es una tarea
del engine, y el autor del workflow lo pone detrás de un `gate`, que ya
existe y espera a una persona. `manual_review` y `justification` se retiran
del documento de tareas: del tipo, de la forma publicada, de la regla que los
ataba y su código, del schema, de spec-tasks y del Contrato §5. Un documento
que los escriba recibe el rechazo de clave desconocida nombrándola.

## Racional

Hecho es lo que corre y pasa, y el agente nunca marca su propio trabajo: un
LLM juzgando si una tarea está completa es la excepción a las dos frases que
sostienen el engine, y D14 la admitió acotada a un nodo que nunca se
construyó. Construirlo sería un diseño propio —una rúbrica, un runner, un
evento de veredicto— para un caso que el `gate` ya cubre con una persona
mirando. Retirarlo no cambia nada observable: esas tareas ya cierran por sus
criterios; cambia lo que el agente tiene que escribir, que es menos.

## Alternativas descartadas

- **Construir el nodo de auditoría**: un juicio LLM de completitud dentro
  del engine, contra su propio principio, para un caso que un `gate` cubre.
- **Registrarlo como deuda** y dejar los campos: seguiría obligando a escribir
  una justificación que nadie lee, y prometiendo un juez que no está.
- **Dejar los campos como documentación de la tarea**, sin regla: una clave
  inerte, que D120 y D121 mandan implementar o retirar.
