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
   una vez agotados los eventos, con el código o la señal de salida del
   proceso y las últimas `STDERR_TAIL_LINES = 20` líneas de stderr; una
   sesión sin proceso propio no responde nada. Una sesión que termina sin
   evento terminal falla con `Failure::SessionDied { adapter, exit }`,
   tipado, que llega al log, a `status`, a `--json` y a la crónica; la
   prosa se produce en el borde.
3. **`doctor --session` abre una sesión real por runner.** Corre el
   workflow de doctor —un nodo `kind: prompt` por runner, con las run
   tools montadas— por el mismo camino que un workflow, en el sandbox de
   `yunta test`, y reporta cada runner con la evidencia del punto 2. Gasta
   un prompt por runner, por eso es opt-in; `doctor` sin la bandera sigue
   siendo gratis.

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
