---
number: D163
title: "La `evidence` de una escalación son hechos etiquetados, no prosa"
status: accepted
revises: []
revised_by: []
---

# D163 — La `evidence` de una escalación son hechos etiquetados, no prosa

El Contrato §5.3 y spec-events §5.18 la declaran desde siempre como estructura
que adjunta el engine desde el log; el código la llevaba como `String`. Pasa a
ser una lista de `{label?, value}`: `label` es opcional porque un hecho como
`exit 1` o `no forge reachable from this machine` ya se nombra solo, mientras
que `400` no dice nada sin `limits.max_tokens_per_run` adelante. La regla que
la acompaña: el `summary` es la *afirmación* y la `evidence` es el *registro*
contra el que se audita, y ninguno repite al otro — un productor que escribía
la causa en los dos dejaba a toda superficie imprimiéndola dos veces bajo un
encabezado que prometía algo nuevo. Una superficie con lugar para una sola
línea compone las dos con `text::aside`; una con lugar para las dos las
encabeza por separado. Evidencia de que la estructura ya estaba: tres
productores la escribían a mano como `"label: value; label: value"` con
separadores propios, y el borde del CLI tenía un filtro que borraba la
evidencia cuando el `summary` la contenía. La lectura es tolerante como la de
`Failure` (D133): untagged, con `Prose` último, así un log anterior a la
estructura trae un string y se lee como el único hecho sin etiqueta que
siempre fue — no hace falta saber qué versión escribió la línea que se está
leyendo. El payload se reemplaza en el lugar por D141. El documento JSON del
CLI sí sube de versión (2 → 3): `decision.evidence` cambió de tipo, y su
propia regla versiona eso.

Descartados: arreglar solo el productor dejando `evidence: String` (construye
otra capa sobre un campo que los documentos ya decidían de otra forma, y no
impide que el próximo productor vuelva a duplicar); mantener el filtro en el
borde (deshace en la presentación lo que el productor hizo mal, y solo
acertaba en una de las ocho formas de escalación — las dos paráfrasis del
forge se le escapaban); un `gate_waiting_v2` (D141 lo descarta antes del
primer tag).
