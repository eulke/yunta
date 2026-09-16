---
number: D116
title: "Globs de scope con separador literal"
status: accepted
revises: []
revised_by: []
---

# D116 — Globs de scope con separador literal

Todo glob de scope se compila con `literal_separator(true)`: `*` no cruza `/`,
`**` sí. `compatibility.md` registra la migración: un workflow que dependía de
la laxitud escribe `**`.

Racional: el scope es un techo de permisos y un techo se lee con la
interpretación más estricta; con la semántica por defecto de `globset` un
scope `src/*.rs` matcheaba `src/deep/nested/file.rs`, más de lo que el autor
declaró. La sintaxis `**` existe justamente para pedir recursión de forma
explícita.

Descartado: conservar la semántica laxa (el scope declarado y el scope
efectivo dejan de coincidir).
