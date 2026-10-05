---
number: D117
title: "Umbral de archivos por expansión de scope como config (`limits.max_expansion_files`)"
status: accepted
revises: []
revised_by: []
---

# D117 — Umbral de archivos por expansión de scope como config (`limits.max_expansion_files`)

El techo de archivos que una expansión de scope puede abarcar se declara en
`limits.max_expansion_files`, con default 5 declarado en un único lugar junto
al resto de `limits:` y congelado en el manifest como cualquier otro límite.

Racional: todo límite que gobierna un run se declara y se congela; un número
fijo en el código no aparece en el manifest y por lo tanto no es auditable
desde el recibo.

Descartado: sin techo (la expansión se vuelve un scope abierto por otra
puerta); conservar la constante en el código.
