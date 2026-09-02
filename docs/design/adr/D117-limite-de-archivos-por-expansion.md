# D117 — Umbral de archivos por expansión de scope como config (`limits.max_expansion_files`)

**Estado:** propuesta.

## Contexto

El engine rechaza una expansión de scope que abarque más de cinco archivos, con el número fijo en el código.

## Decisión propuesta

`limits.max_expansion_files` en config, default 5 declarado en un solo lugar junto al resto de `limits:`, congelado en el manifest como cualquier límite.

## Racional

Todo límite que gobierna un run se declara y se congela; un número en el código no aparece en el manifest y por lo tanto no es auditable desde el recibo.

## Alternativas descartadas

- Sin techo: la expansión se vuelve un scope abierto por otra puerta.
