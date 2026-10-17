---
number: D102
title: "Cadena de hashes del event log, separada de firma (Contrato §3.3)"
status: accepted
revises: []
revised_by: []
---

# D102 — Cadena de hashes del event log, separada de firma (Contrato §3.3)

`event_hash = SHA-256(prev_event_hash || campos estructurales en orden fijo)`,
sobre los bytes persistidos antes de normalización (D70) — la integridad de la
cadena queda ortogonal a la evolución del schema. Génesis `H0 =
SHA-256(manifest_hash)`: determinístico, único por run, sin constante
inventada. Se persiste con el evento, nunca se recalcula en cada replay (costo
O(n) evitado); la verificación es una operación explícita que corre siempre al
generar el recibo (para que "hash-linked" en el recibo sea una afirmación
comprobada, no decorativa — mismo principio de I20) y bajo demanda vía `yunta
verify`. Cierra una promesa de producto (RFC-0003 §1, "342 events,
hash-linked") que no tenía backing normativo.

Descartado: recalcular la cadena en cada resume (caro, redundante con la
verificación de artifacts que §8.1 ya hace) y confundir esto con autenticidad
(la firma es una capa aparte, A-06, y sigue siendo deuda).
