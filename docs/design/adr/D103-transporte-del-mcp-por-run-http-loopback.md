---
number: D103
title: "Transporte del MCP por-run: HTTP loopback efimero, scopeado por token, por sesión de nodo (Contrato §6.5)"
status: accepted
revises: []
revised_by: []
---

# D103 — Transporte del MCP por-run: HTTP loopback efimero, scopeado por token, por sesión de nodo (Contrato §6.5)

El engine es servidor, el adapter traduce el endpoint al mecanismo nativo de
su CLI (mismo patrón que `agent:`/`edit_hooks`), el agente es cliente. HTTP
sobre loopback —no Unix sockets ni named pipes— para portabilidad idéntica en
las tres plataformas sin ramas de código. Ciclo de vida por **sesión de
nodo**, no por run: nace antes del spawn, muere con la sesión, `resume`
siempre emite credencial nueva. Los datos (blackboard, task status) viven en
el storage del run, no en el listener — por eso el blackboard sigue leíble
tras el join aunque el listener que postó ya haya muerto. Scoping por
construcción: el token encapsula `(run_id, node_id, intento)`, ninguna tool
toma un run_id como parámetro — un `get_run(any_id)` genérico no puede existir
porque no hay superficie para pedirlo. Cierra una frontera que tenía tipo
(`SessionRequest.run_tools_endpoint`) pero no mecanismo desde la primera
versión de la Spec del Adapter.
