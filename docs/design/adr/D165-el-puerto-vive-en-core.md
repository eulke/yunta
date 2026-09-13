---
number: D165
title: "El puerto de adapters lo define quien lo consume, en `yunta_core::port`"
status: accepted
revises: []
revised_by: []
---

# D165 — El puerto de adapters lo define quien lo consume, en `yunta_core::port`

`Adapter`, `AgentSession`, `SessionRequest`, `RunToolsEndpoint`, `Budget`,
`PermissionProfile`, `ProbeReport`, `AgentEvent`, `AdapterError` y la tabla
capacidad→política viven en `yunta_core::port`. `yunta_core::process` recibe
la maquinaria de subprocesos (`subprocess`, `signal`, `process_start`).
`yunta-adapters` implementa el puerto y no lo exporta. `yunta-engine` depende
de `yunta-core` y `yunta-storage` únicamente; un test de frontera
(`no_adapter_crate_in_engine`) lo sostiene. El CLI es la raíz de composición
y construye el registro de adapters concretos una vez. `yunta-testkit` se
parte en `yunta-testkit-core` (depende solo de core) y `yunta-testkit`.

Racional: un engine que "conoce a un adapter solo por lo que declara" no
puede depender del crate de adapters concretos para saber qué es un adapter.
Hoy la dirección de dependencias no tiene ciclos, pero la interfaz está del
lado equivocado, y `core` y `adapters` no pueden usar el testkit porque el
testkit depende de ellos.

Descartados: un crate propio `yunta-port` (una superficie más sin consumidor
propio; `Capabilities` ya vive en core, y el semver del puerto es el del
core mientras no haya adapters de terceros publicados — si aparecen, extraer
el crate es un movimiento mecánico desde `core::port`).
