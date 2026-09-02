# D116 — Globs de scope con separador literal

**Estado:** propuesta.

## Contexto

Los globs de `scope` se compilan con la semántica por defecto de `globset`, donde `*` cruza `/`. Un scope `src/*.rs` matchea `src/deep/nested/file.rs`, más de lo que el autor declaró.

## Decisión propuesta

Compilar todo glob de scope con `literal_separator(true)`: `*` no cruza `/`, `**` sí. Nota de migración en `compatibility.md`: los workflows que dependían de la laxitud escriben `**`.

## Racional

El scope es un techo de permisos y un techo se lee con la interpretación más estricta; la sintaxis `**` existe justamente para pedir recursión de forma explícita.

## Alternativas descartadas

- Conservar la semántica laxa: el scope declarado y el scope efectivo dejan de coincidir.
