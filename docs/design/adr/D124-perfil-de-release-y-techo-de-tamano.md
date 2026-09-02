# D124 — Perfil de release y techo de tamaño del binario

**Estado:** propuesta.

## Contexto

El workspace no declara `[profile.release]` y el binario estático pesa 31 MiB. El binario chico es una feature del producto y hoy nada la mide.

## Decisión propuesta

`[profile.release]` con `lto = "fat"`, `codegen-units = 1`, `strip = true` y `panic = "abort"`; un paso en el job musl de CI que mide el binario y falla por encima de un techo declarado. El techo se fija en el tamaño medido tras aplicar el perfil más un 10 %.

## Racional

Cada dependencia erosiona el tamaño y sin medición el deterioro es invisible hasta que alguien lo nota en un `curl | sh`. `panic = "abort"` es coherente con un engine que no atrapa panics: un panic es un bug, no un estado a desenrollar.

## Alternativas descartadas

- Perfil sin techo: mide y no protege.
- `panic = "unwind"`: conserva código de desenrollado que ningún camino del engine usa.
