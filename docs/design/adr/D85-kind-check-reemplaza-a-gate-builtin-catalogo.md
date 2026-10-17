---
number: D85
title: "`kind: check` reemplaza a `gate-builtin`; catálogo cerrado de tres builtins (Contrato §7.1)"
status: accepted
revises: []
revised_by: []
---

# D85 — `kind: check` reemplaza a `gate-builtin`; catálogo cerrado de tres builtins (Contrato §7.1)

El nombre anterior sugería una espera que nunca ocurre: un `gate` bloquea
esperando a una persona (`gate_waiting`, estado `waiting` indefinido); un
check evalúa con datos del engine y sigue o falla en segundos — está mucho más
cerca de un nodo `bash` que de un gate. Vocabulario resultante, coherente con
la distinción que el sistema hace en todos lados: **gate = espera humana,
check = verificación automática**. Builtins, lista cerrada y corta porque un
builtin es por definición algo que el engine ya evalúa con datos que ya tiene
(lo extensible es un `executor`): `baseline_compare`, `coverage_gate` y
`findings_gate` (con `max_severity`, habilitado por D79).

Descartados: un builtin de presupuesto (los límites ya pausan el run solos,
§8.3 — sería redundante), hacer los builtins extensibles (para eso están los
executors) y absorberlos en `bash` (perdería que el engine los evalúe contra
el baseline y los findings del run.dir, que no son comandos cualesquiera).
