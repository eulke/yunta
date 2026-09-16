---
number: D135
title: "Códigos de regla tipados, contados por el par `(kind, código)`"
status: accepted
revises: []
revised_by: []
---

# D135 — Códigos de regla tipados, contados por el par `(kind, código)`

Las reglas que solo se pueden mirar sobre el documento entero declaran su
código en un enum exhaustivo, `RuleCode`, y el problema lo lleva como valor y
no como cadena. Tres códigos son deliberadamente compartidos por las tres
kinds — `duplicate-id`, `empty-title`, `missing-values` —: es una sola regla
preguntada a tres documentos. Lo que distingue una instancia de otra es el
documento, que viaja en el `Report` y llega hasta el recibo, donde la cuenta
de diagnósticos es por par `(ArtifactKind, RuleCode)`.

Racional: un código es vocabulario publicado — se cuenta en el recibo y
sobrevive en el log —, y un vocabulario escrito como literales dispersos se
acuña dos veces sin que nadie se entere: `duplicate-id` ya existía tres veces,
una por documento, y un recibo que contaba por código solo no podía decir de
cuál. El par no obliga a inventar `ledger-duplicate-id`: el kind ya está en el
reporte, y aplanarlo era lo que volvía ambigua la cuenta.

Descartados: un código propio por kind (triplica el vocabulario y deja sin
responder "cuántos ids repetidos hubo en este run"); conservar las cadenas
detrás de una constante por regla (una constante no impide que la cuarta se
escriba a mano en otro archivo).
