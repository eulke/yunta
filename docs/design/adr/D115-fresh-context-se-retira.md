# D115 — `fresh_context` se retira del schema

**Estado:** propuesta.

## Contexto

El schema acepta `fresh_context: true` en nodos `loop` y el engine no lo lee. El comportamiento que describe — cada tarea arranca en una sesión recién nacida — es I8 y ya está garantizado; la única variante real es `on_interrupt: resume_session`, que reutiliza una sesión al reanudar.

## Decisión propuesta

Retirar `fresh_context` del schema; `check` lo rechaza como clave desconocida (D110) con un mensaje que nombra `on_interrupt`.

## Racional

Dos claves para un concepto son una fuente de ambigüedad, y una clave que el engine ignora es una promesa vacía. I8 no necesita opt-in.

## Alternativas descartadas

- Implementarla como sinónimo de `on_interrupt: restart_node`: duplica una clave existente.
