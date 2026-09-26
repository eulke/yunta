---
number: D141
title: "Antes del primer tag publicado, un payload de evento se reemplaza en el lugar"
status: revised
revises: []
revised_by: [D166]
---

# D141 — Antes del primer tag publicado, un payload de evento se reemplaza en el lugar

*(Revisada por D166: la reestructura de payloads por dominio es lo primero que
se hace bajo esta regla, antes del primer tag.)* La regla de §3.1 del Contrato
—quitar, renombrar o cambiar el tipo de un campo de un payload es un `kind`
nuevo— rige desde la primera versión publicada en adelante. Antes de ella el
payload se reemplaza en el lugar, conservando `kind` y `schema_version`: así
cambia `node_failed`, que pasa a llevar `failure` (D133). La evidencia de que
eso no alcanza a nadie está en el repo: no hay ningún tag, y `status.md` sigue
listando el primer `vX.Y.Z` como pendiente — no existe un binario publicado
que haya escrito o leído la forma anterior. Lo que fija la línea es el tag, no
el número de versión: este se mueve entre publicaciones y citarlo dejaría la
decisión envejeciendo sola.

Racional: la regla existe para proteger lectores reales, y aplicarla donde no
hay ninguno deja un `_vN` en el log y en el schema publicado desde el día cero
por un formato que nadie leyó nunca. Dejar dicha la línea de largada es además
lo que evita que el corpus se lea como si el código hubiera violado su propia
regla: una regla de compatibilidad sin fecha de inicio no tiene forma de
distinguir las dos cosas.

Descartados: emitir `node_failed_v2` (arrastra para siempre una variante de
versión por un formato sin lectores); conservar `outcome` al lado de `failure`
como campo puramente aditivo (dos representaciones del mismo hecho sin nada
que las obligue a coincidir, que es el defecto que D133 corrige).
