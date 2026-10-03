---
number: D70
title: "Política de schema de eventos (Contrato §3.2)"
status: accepted
revises: []
revised_by: []
---

# D70 — Política de schema de eventos (Contrato §3.2)

Cuatro reglas: (a) `schema_version` es por tipo de evento, no global — un kind
evoluciona sin arrastrar a los demás; (b) dentro de una versión solo cambios
compatibles (agregar campos opcionales); lo incompatible es un `kind` nuevo
(`x_v2`) que los lectores viejos ignoran; (c) lector tolerante (campos
desconocidos se ignoran, el replay nunca falla por un evento más nuevo) y
escritor estricto (payload inválido = bug del engine); (d) un `kind`
desconocido marca el run como parcialmente interpretado con diagnóstico, jamás
`broken` ni silencio. Forma: JSON Schema **generado desde los tipos de Rust**
y versionado en el repo — los tipos son la fuente de verdad, todo cambio de
formato produce diff visible en el PR, y los golden tests comparan contra el
schema emitido.

Descartada: migración del log al actualizar. Razones: reescribir eventos
convierte la evidencia en "lo que la última versión del migrador cree que
ocurrió" — un migrador correcto y uno con bug producen logs igualmente
plausibles, y por ser invisible no queda forma de distinguirlos; además los
`events.jsonl` ya exportados están fuera del alcance de cualquier migración,
con lo cual la compatibilidad de lectura sigue siendo necesaria de todos modos
(se terminaría pagando las dos cosas). El costo aceptado — variantes `_v2`
conviviendo — está acotado por la retención y por lo raro que será un cambio
incompatible. Se incorpora en cambio la **normalización en lectura**: el
engine convierte cada variante al modelo de dominio actual en memoria, de modo
que ningún `_vN` sea visible en superficies de usuario — vocabulario limpio
sin reescribir un solo byte. Descartada también: schema escrito a mano (se
desincroniza de los tipos en el primer PR apurado).
