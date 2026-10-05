---
number: D114
title: "`runner` en pack manifests, payloads y JSON; `role` solo en prosa"
status: accepted
revises: []
revised_by: []
---

# D114 — `runner` en pack manifests, payloads y JSON; `role` solo en prosa

`pack.yaml` declara `requires.runners`; los payloads del event log y el JSON
de `stats` y del recibo usan `runner`. Los campos son aditivos y el lector
tolerante (D70) sigue leyendo los payloads ya persistidos con el nombre
anterior.

Racional: una palabra, un significado (D27, D85, D87): que el schema y los
datos usen la palabra que el vocabulario reserva para la prosa es una
contradicción que cada lector nuevo tiene que resolver por su cuenta.

Descartado: cambiar el vocabulario (`runner` ya nombra el binding en config y
workflows; el cambio iría en contra de todo lo publicado).
