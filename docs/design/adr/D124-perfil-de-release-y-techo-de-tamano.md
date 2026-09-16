---
number: D124
title: "Perfil de release y techo de tamaño del binario"
status: revised
revises: []
revised_by: [D155]
---

# D124 — Perfil de release y techo de tamaño del binario

*(Revisada por D155: el perfil sigue vigente; el techo deja de proteger el
tamaño y pasa a atajar solo lo enorme.)* `[profile.release]` con `lto =
"fat"`, `codegen-units = 1`, `strip = true` y `panic = "abort"`; el job musl
de CI mide el binario y falla por encima de un techo declarado, fijado en el
tamaño medido tras aplicar el perfil más un 10 %.

Racional: el binario estático chico es una feature del producto; cada
dependencia erosiona el tamaño y sin medición el deterioro es invisible hasta
que alguien lo nota en un `curl | sh`. `panic = "abort"` es coherente con un
engine que no atrapa panics: un panic es un bug, no un estado a desenrollar.

Descartado: perfil sin techo (mide y no protege); `panic = "unwind"` (conserva
código de desenrollado que ningún camino del engine usa).
