---
number: D180
title: "El servidor per-run se llama `yunta-run`; una sesión que muere dice cómo salió; `doctor --session` abre una sesión real"
status: accepted
revises: [D147]
revised_by: []
---

# D180 — El servidor per-run se llama `yunta-run`; una sesión que muere dice cómo salió; `doctor --session` abre una sesión real

## Contexto

Reportado desde Codex: el usuario registra el control plane como
`[mcp_servers.yunta] command = "yunta" args = ["mcp"]`; el adapter inyecta
el servidor per-run como `-c mcp_servers.yunta.url=…` sobre la misma
tabla, y Codex rechaza `url is not supported for stdio`. La sesión muere
antes de su primera línea; el stderr que lo dice se drena a `tracing` y el
nodo falla con «session ended without a terminal event» y 0 tokens;
`doctor` dice sano porque `probe()` corre `--version` y nunca abre una
sesión (plan de raíz, §11 L-106).

## Decisión

1. **El servidor per-run tiene nombre propio.** `RunToolsEndpoint::SERVER_NAME`
   es `yunta-run`; el control plane lo registra el usuario con el nombre
   que quiera. D147 nombraba `mcp_servers.yunta`; esta decisión lo revisa.
2. **Una sesión que muere dice cómo salió.** `AgentSession::exit` responde,
   con el código o la señal de salida del proceso y las últimas
   `STDERR_TAIL_LINES = 20` líneas de stderr; reap el hijo, aborta el
   drenaje y entrega lo que la cola tiene, así que nada espera a un nieto
   que dejó stderr abierto. Se pregunta sólo a la sesión cuyo stream
   terminó sin evento terminal: una que cerró su turno no paga nada, y una
   sin proceso propio responde `None`. La muerte llega tipada por los dos
   caminos que abren sesiones: el nodo de prompt falla con
   `Failure::SessionDied { adapter, exit }` y el ciclo de tareas bloquea
   con `BlockedCause::SessionDied`, de ahí al log, a `status`, a `--json` y
   a la crónica; la prosa se produce en el borde.
3. **`doctor --session` abre una sesión real por runner sano.** Un run por
   runner —un solo nodo `kind: prompt` sobre él, con las run tools
   montadas—, por el mismo camino que un workflow y en el sandbox de
   `yunta test`, con los adapters reales y el entorno de la invocación. Uno
   por runner, y no un run con un nodo por runner, porque la salud del
   adapter rechaza la invocación entera: así un runner que muere se reporta
   como él mismo. Se intentan los runners cuyo adapter ya probó sano;
   los demás los reporta el `doctor` de siempre. Gasta un prompt por
   runner, por eso es opt-in, y `doctor` sin la bandera dice qué garantiza
   —que el binario está, responde y autentica— y qué no.

## Racional

Frontera: dos servidores son dos nombres. Degradación explícita: una
sesión que muere al arrancar deja de llegar como «0 tokens». Un camino de
ejecución: `doctor --session` no abre sesiones por una puerta propia; corre
un run. Veinte líneas de stderr alcanzan para leer el error de arranque de
un CLI y no para que un log de sesión entero viaje en un evento.

## Alternativas descartadas

Avisar en la documentación que el control plane no se registre como
`yunta`: el nombre es el natural y la colisión es nuestra. Emitir un
evento terminal desde el adapter cuando el proceso muere: el adapter nunca
inventa un terminal —el contrato del stream es del engine—; el engine
pregunta y registra. Que `doctor`
pruebe la sesión siempre: gasta tokens sin que nadie lo pida.
