---
number: D181
title: "Dos Ctrl-C: el primero detiene el trabajo, el segundo aborta lo que detenerlo todavía sostiene"
status: accepted
revises: []
revised_by: []
---

# D181 — Dos Ctrl-C: el primero detiene el trabajo, el segundo aborta lo que detenerlo todavía sostiene

## Contexto

M27 pone todo subproceso del CLI bajo el token de la invocación. Lo que
devuelve una toma —`released()` libera el checkout, `hand_over_worktree`
lo entrega al proceso desacoplado— corre *porque* el token disparó, y
corre git: bajo ese mismo token, su git se cancela y el lock queda tomado
por un pid muerto, lo contrario de lo que Ctrl-C promete (plan de raíz,
§11 L-108). Y un comando que no observa nada tiene que seguir muriendo a
un Ctrl-C, como hoy.

## Decisión

La interrupción de una invocación tiene dos etapas. El primer Ctrl-C
detiene el trabajo: dispara el token bajo el que corre todo lo que la
invocación spawneó para trabajar (`Context::supervision`). El segundo
aborta lo que detenerlo todavía sostiene: dispara el token bajo el que
corre lo que devuelve una toma (`Context::teardown`) y devuelve la señal
a la disposición por defecto del proceso. La fuente de la señal se
inyecta en el `Context` —la real en `Context::load`, ninguna en un test,
la del servidor en cada pedido de `yunta mcp`— y el stream se instala
antes de que `load` devuelva.

## Racional

Es el mismo «interrupt, then kill» que el repo aplica a una sesión
(`yunta cancel`, D170 `INTERRUPT_GRACE_PERIOD`): una orden de detenerse
que deja cerrar limpio, y una segunda que no espera. Dueño: nada queda
tomado por un proceso que ya no existe. Núcleo puro: la señal entra
inyectada, y un unit test la dispara sin tocar el proceso.

## Alternativas descartadas

Un solo token para todo: la limpieza muere con el trabajo. Instalar el
listener perezosamente en el primer spawn: el orden de llamadas decide si
`resume` o `mcp` lo tienen, y nada lo prueba. Un timeout para la
limpieza: un umbral más que nadie fijó.
