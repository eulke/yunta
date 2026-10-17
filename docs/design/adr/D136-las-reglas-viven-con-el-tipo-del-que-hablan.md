---
number: D136
title: "Las reglas viven con el tipo del que hablan, y leer un documento las corre"
status: accepted
revises: []
revised_by: []
---

# D136 — Las reglas viven con el tipo del que hablan, y leer un documento las corre

Cada kind interpretada tiene un directorio en `yunta-core` con todo lo que
sabe de sí misma: los tipos, la forma publicada, el recorrido que explica por
qué un documento no deserializó y las reglas que solo valen sobre el documento
entero. El trait `Document` las junta — `KIND`, `EXAMPLE`, `diagnose`, `check`
— y es sellado: las tres kinds son el schema, y una cuarta es un cambio de
schema. La lectura corre las dos pasadas y las reglas en el mismo llamado, y
es la única puerta a un artifact interpretado.

Racional: las reglas son funciones totales y puras de un valor de core a
valores de core — sin reloj, sin ids, sin filesystem —, así que el engine era
el crate equivocado para ellas, y tenerlas afuera hacía de "leer sin validar"
un camino disponible: quien llamaba a la lectura directamente se saltaba en
silencio las siete reglas del ledger, y dos ejecutores releían el mismo
archivo con un parser crudo y mostraban el vocabulario de serde, que es
exactamente lo que D130 existe para eliminar. Con la capa al derecho, un
cambio de schema es un directorio y no una excursión por dos crates.

Descartados: conservar las reglas en el engine y llamarlas después de leer
(dos pasos que el llamador puede olvidar, y los olvidaba); una función
`validate` pública al lado de la lectura (la misma puerta de atrás con otro
nombre); un trait abierto para registrar una kind desde afuera (una kind nueva
mueve el registro de schemas, el recibo y `yunta schema`: es schema, no punto
de extensión).
