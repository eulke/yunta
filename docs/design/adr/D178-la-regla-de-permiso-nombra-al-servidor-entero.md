---
number: D178
title: "La regla de permiso nombra al servidor entero: `mcp__<servidor>__*`"
status: accepted
revises: [D147]
revised_by: []
---

# D178 — La regla de permiso nombra al servidor entero: `mcp__<servidor>__*`

## Contexto

D147 decidió `--allowedTools mcp__yunta` para admitir el servidor per-run
en `claude-code`. Un prefijo de servidor sin `__<tool>` ni `__*` no nombra
ninguna tool: el CLI descarta la regla con un warning de arranque y la
sesión abre sin ninguna de las herramientas del run. El cuerpo de D147 se
enmendó en su lugar para decir `mcp__yunta__*`, sin decisión que lo
registrara ni revisor que lo nombrara (plan de raíz, §11 L-87; M29).

## Decisión

La regla de permiso que un adapter escribe para admitir el servidor
per-run nombra al servidor entero con el comodín del CLI:
`mcp__<servidor>__*`, donde `<servidor>` es `RunToolsEndpoint::SERVER_NAME`.
El adapter no conoce la lista de tools que el engine monta, y no la
necesita. D147 vuelve a decir lo que decidió y esta decisión lo revisa.

## Racional

Una revisión es una decisión: el cuerpo de un ADR no se enmienda; lo que
cambia lo dice un ADR nuevo, y el registro lleva la reciprocidad
(M29). El comodín es lo que hace que el adapter siga sin saber qué tools
monta el engine, que es la frontera que D147 fijó.

## Alternativas descartadas

Enumerar las tools en la regla: obliga al adapter a conocer lo que el
engine monta, y cada tool nueva rompe el permiso. Dejar la enmienda en el
cuerpo: la decisión tal como se tomó sobrevive sólo en git.
