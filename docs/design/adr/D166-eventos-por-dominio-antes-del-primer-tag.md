---
number: D166
title: "Un kind de evento se declara donde vive su dominio, y se reestructura antes del primer tag"
status: accepted
revises: [D141]
revised_by: []
---

# D166 — Un kind de evento se declara donde vive su dominio, y se reestructura antes del primer tag

`EventPayload` pasa a nueve brazos —`Run`, `Node`, `Session`, `Tasks`,
`Scope`, `Findings`, `Artifacts`, `Gates`, `Children`— y cada dominio, en
`core/src/events/<dominio>/`, es dueño de sus kinds, sus payloads con
constructor, su pliegue (`ledger.rs`) y su lectura como `Happening`. El wire
queda byte a byte idéntico: `EventPayloadWire` plano por `#[serde(from/into)]`
(la plantilla es `GateResolvedPayload`), `KINDS` concatenado de los dominios,
`JsonSchema` a mano que emite el mismo `oneOf` de 36 ramas en el mismo orden;
`cargo xtask schema --check` lo guarda. `replay::derive` despacha por
dominio y cada `apply` es exhaustivo; lo que no mueve estado se declara
`Audit` por nombre. Esto se hace antes del primer tag publicado.

Racional: un kind se declara hoy en nueve sitios del workspace y dos docs, y
el compilador defiende tres; `replay::apply` ignora en silencio cualquier kind
que nadie agregue. D141 hace que reestructurar los payloads no cueste ningún
`_v2` mientras no haya tag; después del primer tag, el mismo cambio cuesta uno
por kind para siempre.

Descartados: publicar el tag primero y pagar `_v2` por kind (no compra nada);
mantener la enum plana y agregar macros para derivar `KINDS` (arregla la
declaración y no el pliegue ni el catch-all); sub-enums con wire de dos
niveles (rompe `EventBody::from_object`, todo log exportado y el schema).
